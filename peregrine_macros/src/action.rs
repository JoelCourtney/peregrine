use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};
use syn::{
    Error, Expr, FnArg, GenericArgument, ItemImpl, Pat, Path, PathArguments, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input, parse_quote,
    punctuated::Punctuated,
};

struct ModelMapping {
    model_type: Type,
    _arrow: Token![=>],
    accessor: Expr,
}

impl Parse for ModelMapping {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(ModelMapping {
            model_type: input.parse()?,
            _arrow: input.parse()?,
            accessor: input.parse()?,
        })
    }
}

struct ActAttr {
    mappings: Vec<ModelMapping>,
}

impl Parse for ActAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(ActAttr {
                mappings: Vec::new(),
            });
        }

        // Parse "apply_to"
        let ident: syn::Ident = input.parse()?;
        if ident != "apply_to" {
            return Err(Error::new_spanned(ident, "Expected 'apply_to'"));
        }

        // Parse "="
        input.parse::<Token![=]>()?;

        // Parse braced content
        let content;
        syn::braced!(content in input);

        // Parse comma-separated list of mappings
        let parsed_mappings = Punctuated::<ModelMapping, Token![,]>::parse_terminated(&content)?;

        Ok(ActAttr {
            mappings: parsed_mappings.into_iter().collect(),
        })
    }
}

#[derive(Copy, Clone)]
pub enum ActType {
    Action,
    Activity,
}

pub fn act_attribute(attr: TokenStream, item: TokenStream, act_type: ActType) -> TokenStream {
    let full_trait_path: Path = match act_type {
        ActType::Action => parse_quote! { peregrine::specification::Action },
        ActType::Activity => parse_quote! { peregrine::plan::Activity },
    };

    let erased_trait_path: Path = match act_type {
        ActType::Action => parse_quote! { peregrine::specification::ErasedAction },
        ActType::Activity => parse_quote! { peregrine::plan::ErasedActivity },
    };

    let model_trait_path: Path = match act_type {
        ActType::Action => parse_quote! { peregrine::undo::Undo },
        ActType::Activity => parse_quote! { peregrine::plan::Chronological },
    };

    let assoc_type_tokens = match act_type {
        ActType::Action => quote! { Recorder<'_> },
        ActType::Activity => quote! { ChronoRecorder<'_, '_> },
    };

    let erased_recorder_path: Path = match act_type {
        ActType::Action => parse_quote! { peregrine::undo::ErasedRecorder },
        ActType::Activity => parse_quote! { peregrine::plan::ErasedChronoRecorder },
    };

    let erased_recorder_mapper: TokenStream2 = match act_type {
        ActType::Action => quote! { recorder },
        ActType::Activity => {
            quote! { peregrine::plan::Planner::from_raw_parts(time_tracker, recorder) }
        }
    };

    let erased_apply_time_arg = match act_type {
        ActType::Action => quote! {},
        ActType::Activity => quote! { time_tracker: std::rc::Rc<std::cell::Cell<Time>>, },
    };

    let recorder_shorthand_path: Path = match act_type {
        ActType::Action => parse_quote! { peregrine::undo::Record },
        ActType::Activity => parse_quote! { peregrine::plan::Planner },
    };

    let input = parse_macro_input!(item as ItemImpl);
    let attr = parse_macro_input!(attr as ActAttr);

    let trait_name = full_trait_path.segments.last().unwrap().ident.to_string();

    // Validate that this is an impl block for Action/Activity trait
    let trait_path = match &input.trait_ {
        Some((_, path, _)) => path,
        None => {
            return Error::new_spanned(
                &input,
                format!(
                    "The #[{}] attribute can only be applied to {trait_name} trait implementations",
                    trait_name.to_ascii_lowercase()
                ),
            )
            .to_compile_error()
            .into();
        }
    };

    // Check if it's implementing the Action/Activity trait
    let last_segment = match trait_path.segments.last() {
        Some(seg) if seg.ident == trait_name => seg,
        _ => {
            return Error::new_spanned(
                trait_path,
                format!(
                    "The #[{}] attribute can only be applied to {trait_name} trait implementations",
                    trait_name.to_ascii_lowercase()
                ),
            )
            .to_compile_error()
            .into();
        }
    };

    // Extract the type being implemented
    let self_ty = &input.self_ty;

    // Extract the Model type from the generic parameter Action<Model>
    let model_type = match &last_segment.arguments {
        PathArguments::AngleBracketed(args) => {
            if args.args.len() != 1 {
                return Error::new_spanned(
                    &last_segment.arguments,
                    format!("The {trait_name} trait should have exactly one generic parameter"),
                )
                .to_compile_error()
                .into();
            }
            match args.args.first() {
                Some(GenericArgument::Type(ty)) => ty,
                _ => {
                    return Error::new_spanned(
                        &last_segment.arguments,
                        format!("{trait_name} trait parameter should be a type"),
                    )
                    .to_compile_error()
                    .into();
                }
            }
        }
        _ => {
            return Error::new_spanned(
                last_segment,
                format!(
                    "{trait_name} trait must have a generic parameter, e.g., {trait_name}<MyModel>"
                ),
            )
            .to_compile_error()
            .into();
        }
    };

    // Extract generics from the impl block
    let impl_generics = &input.generics;
    let (impl_gen, _, where_clause) = impl_generics.split_for_impl();

    // Extract model argument name from apply function
    let model_arg_ident = match input.items.first().unwrap() {
        syn::ImplItem::Fn(f) => {
            let second_arg = f.sig.inputs.get(1).unwrap();
            match second_arg {
                FnArg::Typed(pat_type) => match &*pat_type.pat {
                    Pat::Ident(ident) => ident.ident.clone(),
                    _ => {
                        return Error::new_spanned(pat_type, "Expected identifier pattern")
                            .to_compile_error()
                            .into();
                    }
                },
                _ => unreachable!(),
            }
        }
        _ => unreachable!(),
    };

    let accessor_mapper: Box<dyn Fn(TokenStream2) -> TokenStream2> = match act_type {
        ActType::Action => Box::new(|i| quote! { &mut #i }),
        ActType::Activity => {
            Box::new(|i| quote! { #model_arg_ident.map_model(|#model_arg_ident| &mut #i)})
        }
    };

    // Generate delegating Action implementations for each mapping
    let delegating_impls = attr.mappings.iter().map(|mapping| {
        let target_model = &mapping.model_type;
        let accessor = &mapping.accessor;

        let mapped_accessor = accessor_mapper(accessor.into_token_stream());

        quote! {
            impl #impl_gen #full_trait_path<#target_model> for #self_ty #where_clause {
                fn apply(&self, #model_arg_ident: #recorder_shorthand_path<#target_model>) {
                    <Self as #full_trait_path<#model_type>>::apply(self, #mapped_accessor);
                }
            }
        }
    });

    // Collect all model type IDs (original + mappings)
    let all_model_types = {
        let mut types = vec![model_type.clone()];
        types.extend(attr.mappings.iter().map(|m| m.model_type.clone()));
        types
    };

    // Generate match arms for apply_by_id
    let apply_by_id_arms = all_model_types.iter().map(|model_ty| {
        quote! {
            if model_type_id == ::std::any::TypeId::of::<#model_ty>() {
                // SAFETY: none
                // It is the responsibility of the caller to provide the correct recorder
                // corresponding to the model type id.
                let recorder = unsafe {
                    &mut *(#model_arg_ident as *mut dyn #erased_recorder_path
                        as *mut <#model_ty as #model_trait_path>::#assoc_type_tokens)
                };

                <Self as #full_trait_path<#model_ty>>::apply(self, #erased_recorder_mapper);
                return;
            }
        }
    });

    // Generate the ErasedAction implementation
    let erased_impl = quote! {
        #[peregrine::macro_prelude::typetag::serde]
        impl #impl_gen #erased_trait_path for #self_ty #where_clause {
            fn model_type_ids(&self) -> ::std::vec::Vec<::std::any::TypeId> {
                vec![#(::std::any::TypeId::of::<#all_model_types>()),*]
            }

            unsafe fn apply_by_id(
                &self,
                model_type_id: ::std::any::TypeId,
                #erased_apply_time_arg
                #model_arg_ident: &mut dyn #erased_recorder_path,
            ) {
                #(#apply_by_id_arms)*

                panic!(
                    "Type ID mismatch in apply_by_id: got {:?}, expected one of [{}]",
                    model_type_id,
                    [#(::std::any::TypeId::of::<#all_model_types>()),*]
                        .iter()
                        .map(|id| format!("{:?}", id))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
    };

    // Combine the original impl block with the generated implementations
    let expanded = quote! {
        #input

        #(#delegating_impls)*

        #erased_impl
    };

    TokenStream::from(expanded)
}
