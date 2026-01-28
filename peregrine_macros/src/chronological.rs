use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Error, Fields, parse_macro_input};

pub fn derive_chronological(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // Validate that this is a struct
    let Data::Struct(data) = input.data else {
        return Error::new_spanned(input, "Chronological can only be derived for structs")
            .to_compile_error()
            .into();
    };

    let struct_name = &input.ident;
    let visibility = &input.vis;
    let generics = &input.generics;

    let chrono_recorder_name = format_ident!("{}ChronoRecorder", &struct_name);

    // Extract only the generic parameters (without bounds)
    let generic_params = &generics.params;

    // Process fields
    let (chrono_recorder_fields, chrono_recorder_inits) = match &data.fields {
        Fields::Named(fields) => {
            let mut chrono_recorder_fields = Vec::new();
            let mut chrono_recorder_inits = Vec::new();

            for (index, field) in fields.named.iter().enumerate() {
                let field_type = &field.ty;
                let field_vis = &field.vis;
                let field_name = if let Some(i) = &field.ident {
                    i.clone()
                } else {
                    format_ident!("{index}")
                };

                // Check if field has #[no_undo] or #[no_chrono] attribute
                let has_no_undo_attr = field
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("no_undo"));
                let has_no_chrono_attr = field
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("no_chrono"));

                if !has_no_undo_attr && !has_no_chrono_attr {
                    // Field is not excluded
                    chrono_recorder_fields.push(quote! {
                        #field_vis #field_name: <#field_type as peregrine::plan::Chronological>::ChronoRecorder<'r, 'i>
                    });

                    // Add chrono_recorder initialization
                    chrono_recorder_inits.push(quote! {
                        #field_name: <#field_type as peregrine::plan::Chronological>::chrono_recorder(
                            time,
                            &mut recorder.#field_name
                        )
                    });
                }
            }

            (chrono_recorder_fields, chrono_recorder_inits)
        }
        Fields::Unnamed(_) => {
            return Error::new_spanned(
                data.fields,
                "Chronological derive macro does not support tuple structs",
            )
            .to_compile_error()
            .into();
        }
        Fields::Unit => {
            return Error::new_spanned(
                data.fields,
                "Chronological derive macro does not support unit structs",
            )
            .to_compile_error()
            .into();
        }
    };

    // Split generics for impl block
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let expanded = quote! {
        #visibility struct #chrono_recorder_name<'r, 'i, #ty_generics> where 'i: 'r #where_clause {
            #(#chrono_recorder_fields,)*
        }

        impl<'r, 'i, #generic_params> peregrine::plan::ErasedChronoRecorder for #chrono_recorder_name<'r, 'i, #generic_params> where 'i: 'r #where_clause {}

        impl #impl_generics peregrine::plan::Chronological for #struct_name #ty_generics #where_clause {
            type ChronoRecorder<'r, 'i> = #chrono_recorder_name<'r, 'i, #generic_params>
            where
                Self: 'r + 'i, 'i: 'r;

            fn chrono_recorder<'r, 'i>(
                time: &std::rc::Rc<std::cell::Cell<peregrine::plan::Time>>,
                recorder: &'r mut Self::Recorder<'i>,
            ) -> Self::ChronoRecorder<'r, 'i> where 'i: 'r {
                #chrono_recorder_name {
                    #(#chrono_recorder_inits,)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}
