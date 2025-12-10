use heck::ToUpperCamelCase;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Error, Fields, parse_macro_input};

pub fn derive_undo(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // Validate that this is a struct
    let Data::Struct(data) = input.data else {
        return Error::new_spanned(input, "Undo can only be derived for structs")
            .to_compile_error()
            .into();
    };

    let struct_name = &input.ident;
    let visibility = &input.vis;
    let generics = &input.generics;

    // Convert struct name to snake_case for module name
    let recorder_name = format_ident!("{}Recorder", &struct_name);
    let record_id_name = format_ident!("{}RecordId", &struct_name);

    // Extract only the generic parameters (without bounds)
    let generic_params = &generics.params;

    // Process fields
    let (recorder_fields, record_id_variants, recorder_inits, remove_record_arms, iter_chains) =
        match &data.fields {
            Fields::Named(fields) => {
                let mut recorder_fields = Vec::new();
                let mut record_id_variants = Vec::new();
                let mut recorder_inits = Vec::new();
                let mut remove_record_arms = Vec::new();
                let mut iter_chains = Vec::new();

                for (index, field) in fields.named.iter().enumerate() {
                    let field_type = &field.ty;
                    let field_vis = &field.vis;
                    let (field_name, variant_name) = if let Some(i) = &field.ident {
                        (
                            i.clone(),
                            format_ident!("{}", i.to_string().to_upper_camel_case()),
                        )
                    } else {
                        (format_ident!("{index}"), format_ident!("Field{index}"))
                    };

                    // Check if field has #[undo] attribute
                    let has_undo_attr = field.attrs.iter().any(|attr| attr.path().is_ident("undo"));

                    if has_undo_attr {
                        // Field is annotated with #[undo]
                        recorder_fields.push(quote! {
                            #field_vis #field_name: <#field_type as Undo>::Recorder<'rec>
                        });

                        // Add variant to RecordId enum
                        record_id_variants.push(quote! {
                            #variant_name(<#field_type as Undo>::RecordId)
                        });

                        // Add recorder initialization (call .recorder() on the field)
                        recorder_inits.push(quote! {
                            #field_name: self.#field_name.recorder()
                        });

                        // Add match arm for remove_record
                        remove_record_arms.push(quote! {
                            #record_id_name::#variant_name(id) => self.#field_name.remove_record(id)
                        });

                        // Add iterator chain for IntoAnonIterator
                        iter_chains.push(quote! {
                            self.#field_name.into_anon_iter().map(#record_id_name::#variant_name)
                        });
                    } else {
                        // Field is NOT annotated
                        recorder_fields.push(quote! {
                            #field_vis #field_name: &'rec #field_type
                        });

                        // Add recorder initialization (borrow the field)
                        recorder_inits.push(quote! {
                            #field_name: &self.#field_name
                        });
                    }
                }

                (
                    recorder_fields,
                    record_id_variants,
                    recorder_inits,
                    remove_record_arms,
                    iter_chains,
                )
            }
            Fields::Unnamed(_) => {
                return Error::new_spanned(
                    data.fields,
                    "Undo derive macro does not support tuple structs",
                )
                .to_compile_error()
                .into();
            }
            Fields::Unit => {
                return Error::new_spanned(
                    data.fields,
                    "Undo derive macro does not support unit structs",
                )
                .to_compile_error()
                .into();
            }
        };

    // Split generics for impl block
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Build the iterator chain expression
    let into_anon_iter_body = if iter_chains.is_empty() {
        quote! { std::iter::empty() }
    } else if iter_chains.len() == 1 {
        let chain = &iter_chains[0];
        quote! { #chain }
    } else {
        let first = &iter_chains[0];
        let rest = &iter_chains[1..];
        quote! {
            #first
            #(.chain(#rest))*
        }
    };

    let expanded = quote! {
        #visibility struct #recorder_name<'rec, #generic_params> {
            #(#recorder_fields,)*
        }

        #visibility enum #record_id_name<#generic_params> {
            #(#record_id_variants,)*
            _Phantom {
                types: std::marker::PhantomData<#struct_name<#generic_params>>,
                never: Never
            }
        }

        impl<#generic_params> desparrow::undo::IntoAnonIterator for #recorder_name<'_, #generic_params> {
            type Item = #record_id_name<#generic_params>;

            fn into_anon_iter(self) -> impl Iterator<Item = Self::Item> {
                #into_anon_iter_body
            }
        }

        impl #impl_generics desparrow::undo::Undo for #struct_name #ty_generics #where_clause {
            type Recorder<'rec> = #recorder_name<'rec, #generic_params> where Self: 'rec;
            type RecordId = #record_id_name<#generic_params>;

            fn recorder(&mut self) -> Self::Recorder<'_> {
                #recorder_name {
                    #(#recorder_inits,)*
                }
            }

            fn remove_record(&mut self, id: Self::RecordId) {
                match id {
                    #(#remove_record_arms,)*
                    #record_id_name::_Phantom { never, .. } => unreachable!()
                }
            }
        }
    };

    TokenStream::from(expanded)
}
