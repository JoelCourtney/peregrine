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

struct ActivityAttr {
    mappings: Vec<ModelMapping>,
}

impl Parse for ActivityAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(ActivityAttr { mappings: Vec::new() });
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

        Ok(ActivityAttr {
            mappings: parsed_mappings.into_iter().collect(),
        })
    }
}

pub fn activity_attribute(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);
    let attr = parse_macro_input!(attr as ActivityAttr);

    // Validate that this is an impl block for Activity trait
    let trait_path = match &input.trait_ {
        Some((_, path, _)) => path,
        None => {
            return Error::new_spanned(&input, "The #[activity] attribute can only be applied to trait impl blocks")
                .to_compile_error()
                .into();
        }
    };

    // Check if it's implementing the Activity trait
    let last_segment = match trait_path.segments.last() {
        Some(seg) if seg.ident == "Activity" => seg,
        _ => {
            return Error::new_spanned(trait_path, "The #[activity] attribute can only be applied to Activity trait implementations")
                .to_compile_error()
                .into();
        }
    };

    // Extract the type being implemented
    let self_ty = &input.self_ty;

    // Extract the Model type from the generic parameter Activity<Model>
    let model_type = match &last_segment.arguments {
        PathArguments::AngleBracketed(args) => {
            if args.args.len() != 1 {
                return Error::new_spanned(
                    &last_segment.arguments,
                    "Activity trait should have exactly one generic parameter"
                )
                .to_compile_error()
                .into();
            }
            match args.args.first() {
                Some(GenericArgument::Type(ty)) => ty,
                _ => {
                    return Error::new_spanned(
                        &last_segment.arguments,
                        "Activity trait parameter should be a type"
                    )
                    .to_compile_error()
                    .into();
                }
            }
        }
        _ => {
            return Error::new_spanned(
                last_segment,
                "Activity trait must have a generic parameter, e.g., Activity<MyModel>"
            )
            .to_compile_error()
            .into();
        }
    };

    // Extract generics from the impl block
    let impl_generics = &input.generics;
    let (impl_gen, _, where_clause) = impl_generics.split_for_impl();

    // Generate delegating Activity implementations for each mapping
    let delegating_impls = attr.mappings.iter().map(|mapping| {
        let target_model = &mapping.model_type;
        let accessor = &mapping.accessor;

        quote! {
            impl #impl_gen desparrow::plan::Activity<#target_model> for #self_ty #where_clause {
                fn apply(&self, time: desparrow::plan::Time, model: desparrow::undo::Record<#target_model>) {
                    <Self as desparrow::plan::Activity<#model_type>>::apply(self, time, &mut #accessor);
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
                    &mut *(model as *mut dyn desparrow::undo::ErasedRecorder
                        as *mut <#model_ty as desparrow::undo::Undo>::Recorder<'_>)
                };

                <Self as desparrow::plan::Activity<#model_ty>>::apply(self, time, recorder);
                return;
            }
        }
    });

    // Generate the ErasedActivity implementation
    let erased_impl = quote! {
        #[typetag::serde]
        impl #impl_gen desparrow::plan::ErasedActivity for #self_ty #where_clause {
            fn model_type_ids(&self) -> ::std::vec::Vec<::std::any::TypeId> {
                vec![#(::std::any::TypeId::of::<#all_model_types>()),*]
            }

            unsafe fn apply_by_id(
                &self,
                time: desparrow::plan::Time,
                model_type_id: ::std::any::TypeId,
                model: &mut dyn desparrow::undo::ErasedRecorder,
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
