use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Error, Expr, GenericArgument, ItemImpl, PathArguments, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
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

struct ActionAttr {
    mappings: Vec<ModelMapping>,
}

impl Parse for ActionAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(ActionAttr { mappings: Vec::new() });
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

        Ok(ActionAttr {
            mappings: parsed_mappings.into_iter().collect(),
        })
    }
}

pub fn action_attribute(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);
    let attr = parse_macro_input!(attr as ActionAttr);

    // Validate that this is an impl block for Action trait
    let trait_path = match &input.trait_ {
        Some((_, path, _)) => path,
        None => {
            return Error::new_spanned(&input, "The #[action] attribute can only be applied to trait impl blocks")
                .to_compile_error()
                .into();
        }
    };

    // Check if it's implementing the Action trait
    let last_segment = match trait_path.segments.last() {
        Some(seg) if seg.ident == "Action" => seg,
        _ => {
            return Error::new_spanned(trait_path, "The #[action] attribute can only be applied to Action trait implementations")
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
                    "Action trait should have exactly one generic parameter"
                )
                .to_compile_error()
                .into();
            }
            match args.args.first() {
                Some(GenericArgument::Type(ty)) => ty,
                _ => {
                    return Error::new_spanned(
                        &last_segment.arguments,
                        "Action trait parameter should be a type"
                    )
                    .to_compile_error()
                    .into();
                }
            }
        }
        _ => {
            return Error::new_spanned(
                last_segment,
                "Action trait must have a generic parameter, e.g., Action<MyModel>"
            )
            .to_compile_error()
            .into();
        }
    };

    // Extract generics from the impl block
    let impl_generics = &input.generics;
    let (impl_gen, _, where_clause) = impl_generics.split_for_impl();

    // Generate delegating Action implementations for each mapping
    let delegating_impls = attr.mappings.iter().map(|mapping| {
        let target_model = &mapping.model_type;
        let accessor = &mapping.accessor;

        quote! {
            impl #impl_gen peregrine::specification::Action<#target_model> for #self_ty #where_clause {
                fn apply(&self, model: peregrine::undo::Record<#target_model>) {
                    <Self as peregrine::specification::Action<#model_type>>::apply(self, &mut #accessor);
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
                    &mut *(model as *mut dyn peregrine::undo::ErasedRecorder
                        as *mut <#model_ty as peregrine::undo::Undo>::Recorder<'_>)
                };

                <Self as peregrine::specification::Action<#model_ty>>::apply(self, recorder);
                return;
            }
        }
    });

    // Generate the ErasedAction implementation
    let erased_impl = quote! {
        #[peregrine::macro_prelude::typetag::serde]
        impl #impl_gen peregrine::specification::ErasedAction for #self_ty #where_clause {
            fn model_type_ids(&self) -> ::std::vec::Vec<::std::any::TypeId> {
                vec![#(::std::any::TypeId::of::<#all_model_types>()),*]
            }

            unsafe fn apply_by_id(
                &self,
                model_type_id: ::std::any::TypeId,
                model: &mut dyn peregrine::undo::ErasedRecorder,
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