use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};

pub fn generate_struct_impl(
    name: &syn::Ident,
    fields: &syn::Fields,
    impl_generics: syn::ImplGenerics,
    ty_generics: syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    hash_if_expr: Option<proc_macro2::TokenStream>,
) -> TokenStream2 {
    let mut is_hashable_checks = Vec::new();
    let mut hash_unchecked_calls = Vec::new();

    match fields {
        syn::Fields::Named(named_fields) => {
            for field in &named_fields.named {
                let field_name = field
                    .ident
                    .as_ref()
                    .expect("Named field should have an identifier");

                // Check if field has #[always_hash] attribute
                let has_always_hash = field
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("always_hash"));

                if has_always_hash {
                    // For #[always_hash] fields, skip is_hashable check and use normal Hash
                    hash_unchecked_calls.push(quote! {
                        {
                            use std::hash::Hash;
                            self.#field_name.hash(state);
                        }
                    });
                } else {
                    // For regular fields, delegate to MaybeHash implementation
                    is_hashable_checks.push(quote! {
                        self.#field_name.is_hashable()
                    });
                    hash_unchecked_calls.push(quote! {
                        self.#field_name.hash_unchecked(state);
                    });
                }
            }
        }
        syn::Fields::Unnamed(unnamed_fields) => {
            for (i, field) in unnamed_fields.unnamed.iter().enumerate() {
                let field_index = syn::Index::from(i);

                // Check if field has #[always_hash] attribute
                let has_always_hash = field
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("always_hash"));

                if has_always_hash {
                    // For #[always_hash] fields, skip is_hashable check and use normal Hash
                    hash_unchecked_calls.push(quote! {
                        {
                            use std::hash::Hash;
                            self.#field_index.hash(state);
                        }
                    });
                } else {
                    // For regular fields, delegate to MaybeHash implementation
                    is_hashable_checks.push(quote! {
                        self.#field_index.is_hashable()
                    });
                    hash_unchecked_calls.push(quote! {
                        self.#field_index.hash_unchecked(state);
                    });
                }
            }
        }
        syn::Fields::Unit => {
            // Unit structs have no fields, so no checks or calls needed
        }
    }

    let is_hashable_body = if let Some(expr) = hash_if_expr {
        if is_hashable_checks.is_empty() {
            quote! { #expr }
        } else {
            quote! {
                #expr && (#(#is_hashable_checks)&&*)
            }
        }
    } else if is_hashable_checks.is_empty() {
        quote! { true }
    } else {
        quote! {
            #(#is_hashable_checks)&&*
        }
    };

    quote! {
        impl #impl_generics peregrine::MaybeHash for #name #ty_generics #where_clause {
            fn is_hashable(&self) -> bool {
                #is_hashable_body
            }
            fn hash_unchecked<H: std::hash::Hasher>(&self, state: &mut H) {
                #(#hash_unchecked_calls)*
            }
        }
    }
}

pub fn generate_enum_impl(
    name: &syn::Ident,
    variants: &[&syn::Variant],
    impl_generics: syn::ImplGenerics,
    ty_generics: syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    hash_if_expr: Option<proc_macro2::TokenStream>,
) -> TokenStream2 {
    let mut match_arms_is_hashable = Vec::new();
    let mut match_arms_hash_unchecked = Vec::new();

    for variant in variants {
        let variant_name = &variant.ident;

        match &variant.fields {
            syn::Fields::Named(fields) => {
                // Named fields variant
                let field_names: Vec<_> = fields
                    .named
                    .iter()
                    .map(|f| {
                        f.ident
                            .as_ref()
                            .expect("Named field should have an identifier")
                    })
                    .collect();

                let mut field_is_hashable_checks = Vec::new();
                let mut field_hash_calls = Vec::new();

                for field in &fields.named {
                    let field_name = field
                        .ident
                        .as_ref()
                        .expect("Named field should have an identifier");

                    let has_always_hash = field
                        .attrs
                        .iter()
                        .any(|attr| attr.path().is_ident("always_hash"));

                    if has_always_hash {
                        field_hash_calls.push(quote! {
                            {
                                use std::hash::Hash;
                                #field_name.hash(state);
                            }
                        });
                    } else {
                        field_is_hashable_checks.push(quote! {
                            #field_name.is_hashable()
                        });
                        field_hash_calls.push(quote! {
                            #field_name.hash_unchecked(state);
                        });
                    }
                }

                let is_hashable_body = if field_is_hashable_checks.is_empty() {
                    quote! { true }
                } else {
                    quote! {
                        #(#field_is_hashable_checks)&&*
                    }
                };

                match_arms_is_hashable.push(quote! {
                    #name::#variant_name { #(#field_names),* } => #is_hashable_body
                });

                match_arms_hash_unchecked.push(quote! {
                    #name::#variant_name { #(#field_names),* } => {
                        use std::hash::Hash;
                        std::mem::discriminant(self).hash(state);
                        #(#field_hash_calls)*
                    }
                });
            }
            syn::Fields::Unnamed(fields) => {
                // Unnamed fields variant
                let field_identifiers: Vec<_> = (0..fields.unnamed.len())
                    .map(|i| format_ident!("field_{}", i))
                    .collect();

                let mut field_is_hashable_checks = Vec::new();
                let mut field_hash_calls = Vec::new();

                for (i, field) in fields.unnamed.iter().enumerate() {
                    let field_ident = format_ident!("field_{}", i);

                    let has_always_hash = field
                        .attrs
                        .iter()
                        .any(|attr| attr.path().is_ident("always_hash"));

                    if has_always_hash {
                        field_hash_calls.push(quote! {
                            {
                                use std::hash::Hash;
                                #field_ident.hash(state);
                            }
                        });
                    } else {
                        field_is_hashable_checks.push(quote! {
                            #field_ident.is_hashable()
                        });
                        field_hash_calls.push(quote! {
                            #field_ident.hash_unchecked(state);
                        });
                    }
                }

                let is_hashable_body = if field_is_hashable_checks.is_empty() {
                    quote! { true }
                } else {
                    quote! {
                        #(#field_is_hashable_checks)&&*
                    }
                };

                match_arms_is_hashable.push(quote! {
                    #name::#variant_name(#(#field_identifiers),*) => #is_hashable_body
                });

                match_arms_hash_unchecked.push(quote! {
                    #name::#variant_name(#(#field_identifiers),*) => {
                        use std::hash::Hash;
                        std::mem::discriminant(self).hash(state);
                        #(#field_hash_calls)*
                    }
                });
            }
            syn::Fields::Unit => {
                // Unit variant
                match_arms_is_hashable.push(quote! {
                    #name::#variant_name => true
                });

                match_arms_hash_unchecked.push(quote! {
                    #name::#variant_name => {
                        use std::hash::Hash;
                        std::mem::discriminant(self).hash(state);
                    }
                });
            }
        }
    }

    let is_hashable_body = if let Some(expr) = hash_if_expr {
        quote! {
            #expr && match self {
                #(#match_arms_is_hashable),*
            }
        }
    } else {
        quote! {
            match self {
                #(#match_arms_is_hashable),*
            }
        }
    };

    quote! {
        impl #impl_generics peregrine::MaybeHash for #name #ty_generics #where_clause {
            fn is_hashable(&self) -> bool {
                #is_hashable_body
            }
            fn hash_unchecked<H: std::hash::Hasher>(&self, state: &mut H) {
                match self {
                    #(#match_arms_hash_unchecked),*
                }
            }
        }
    }
}
