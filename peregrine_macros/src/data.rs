use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{DeriveInput, Fields, Generics, Ident, Variant};

/// Main entry point for the Data derive macro implementation
pub fn generate_data_impl(input: DeriveInput) -> TokenStream {
    let name = &input.ident;
    let mut modified_generics = input.generics.clone();
    modified_generics
        .params
        .push(syn::GenericParam::Lifetime(syn::LifetimeParam {
            lifetime: syn::Lifetime::new("'h", Span::call_site()),
            colon_token: None,
            bounds: syn::punctuated::Punctuated::new(),
            attrs: vec![],
        }));
    let (modified_impl_generics, modified_ty_generics, _) = modified_generics.split_for_impl();
    let (_, ty_generics, where_clause) = input.generics.split_for_impl();

    let sample_type = parse_sample_attribute(&input);

    // Check if the type has fields
    let has_fields = match &input.data {
        syn::Data::Struct(data) => !data.fields.is_empty(),
        syn::Data::Enum(data) => data.variants.iter().any(|v| !v.fields.is_empty()),
        syn::Data::Union(_) => {
            return syn::Error::new_spanned(
                &input.ident,
                "Data derive macro does not support unions",
            )
            .to_compile_error()
            .into();
        }
    };

    if !has_fields {
        let expanded = quote! {
            impl #modified_impl_generics peregrine::Data<'h> for #name #ty_generics #where_clause {
                type Read = Self;
                type Sample = Self;

                fn to_read(&self, _written: peregrine::Time) -> Self::Read { *self }
                fn from_read(read: Self, _written: peregrine::Time) -> Self { read }
                fn sample(read: Self::Read, _now: peregrine::Time) -> Self::Sample { read }
            }
        };
        return TokenStream::from(expanded);
    }

    let (fields, variants, is_struct) = match &input.data {
        syn::Data::Struct(data) => (&data.fields, vec![], true),
        syn::Data::Enum(data) => (
            &Fields::Unit,
            data.variants.iter().cloned().collect(),
            false,
        ),
        syn::Data::Union(_) => unreachable!(),
    };

    let visibility = &input.vis;
    let read_type_name = format_ident!("{}Read", name);
    let is_self_sample = sample_type.as_ref().map(|s| s == "Self").unwrap_or(false);

    if is_self_sample {
        let read_type = if is_struct {
            generate_struct_type(
                &read_type_name,
                fields,
                visibility,
                quote! { #[derive(Clone)] },
                quote! { Read },
                &modified_generics,
                where_clause,
                true,
            )
        } else {
            generate_enum_type(
                &read_type_name,
                &variants,
                visibility,
                quote! { #[derive(Clone)] },
                quote! { Read },
                &modified_generics,
                where_clause,
                true,
            )
        };
        let sample_body = quote! { Self::from_read(read, now) };
        let data_impl = generate_data_methods(
            name,
            fields,
            &variants,
            &read_type_name,
            sample_body,
            is_struct,
        );
        return quote! {
            #read_type

            impl #modified_impl_generics peregrine::Data<'h> for #name #ty_generics #where_clause {
                type Read = #read_type_name #modified_ty_generics;
                type Sample = Self;

                #data_impl
            }
        }
        .into();
    }

    let sample_type_name = sample_type
        .as_ref()
        .map(|s| format_ident!("{}", s))
        .unwrap_or_else(|| format_ident!("{}Sample", name));

    let (read_type, sample_type_def) = if is_struct {
        let read_type = generate_struct_type(
            &read_type_name,
            fields,
            visibility,
            quote! { #[derive(Clone)] },
            quote! { Read },
            &modified_generics,
            where_clause,
            true,
        );
        let sample_type = generate_struct_type(
            &sample_type_name,
            fields,
            visibility,
            quote! { #[derive(peregrine::MaybeHash)] },
            quote! { Sample },
            &modified_generics,
            where_clause,
            false,
        );
        (read_type, sample_type)
    } else {
        let read_type = generate_enum_type(
            &read_type_name,
            &variants,
            visibility,
            quote! { #[derive(Clone)] },
            quote! { Read },
            &modified_generics,
            where_clause,
            true,
        );
        let sample_type = generate_enum_type(
            &sample_type_name,
            &variants,
            visibility,
            quote! { #[derive(peregrine::MaybeHash)] },
            quote! { Sample },
            &modified_generics,
            where_clause,
            false,
        );
        (read_type, sample_type)
    };

    let sample_body = if is_struct {
        generate_struct_field_operations(
            fields,
            &sample_type_name,
            |field_name, field_type| quote! { #field_name: <#field_type as peregrine::Data<'h>>::sample(read.#field_name, now) },
            |field_index, field_type| quote! { <#field_type as peregrine::Data<'h>>::sample(read.#field_index, now) },
        )
    } else {
        generate_enum_operations(
            &read_type_name,
            &variants,
            &sample_type_name,
            quote! { read },
            |field_name, field_type| quote! { #field_name: <#field_type as peregrine::Data<'h>>::sample(#field_name, now) },
            |field_name, field_type| quote! { <#field_type as peregrine::Data<'h>>::sample(#field_name, now) },
        )
    };

    let data_impl = generate_data_methods(
        name,
        fields,
        &variants,
        &read_type_name,
        sample_body,
        is_struct,
    );
    quote! {
        #read_type
        #sample_type_def

        impl #modified_impl_generics peregrine::Data<'h> for #name #ty_generics #where_clause {
            type Read = #read_type_name #modified_ty_generics;
            type Sample = #sample_type_name #modified_ty_generics;

            #data_impl
        }
    }
    .into()
}

/// Extract sample type from #[sample = "TypeName"] or #[sample = Self] attribute
fn parse_sample_attribute(input: &DeriveInput) -> Option<String> {
    for attr in &input.attrs {
        if attr.path().is_ident("sample") {
            if let Ok(syn::Expr::Lit(expr_lit)) = attr.parse_args() {
                if let syn::Lit::Str(lit_str) = expr_lit.lit {
                    return Some(lit_str.value());
                }
            }
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
/// Generate type definitions for structs
fn generate_struct_type(
    type_name: &Ident,
    fields: &Fields,
    visibility: &syn::Visibility,
    derive: TokenStream2,
    associated_type: TokenStream2,
    ty_generics: &Generics,
    where_clause: Option<&syn::WhereClause>,
    generate_copy: bool,
) -> TokenStream2 {
    let field_tokens = generate_field_types(fields, associated_type);
    let copy = if generate_copy {
        let (impl_generics, type_generics, where_clause) = ty_generics.split_for_impl();
        quote! {
            impl #impl_generics Copy for #type_name #type_generics #where_clause {}
        }
    } else {
        quote! {}
    };
    match fields {
        Fields::Named(_) => quote! {
            #derive
            #visibility struct #type_name #ty_generics #where_clause {
                #field_tokens
            }
            #copy
        },
        Fields::Unnamed(_) => quote! {
            #derive
            #visibility struct #type_name #ty_generics #where_clause(#field_tokens);
            #copy
        },
        Fields::Unit => quote! {
            #derive
            #visibility struct #type_name #ty_generics #where_clause;
            #copy
        },
    }
}

#[allow(clippy::too_many_arguments)]
/// Generate type definitions for enums
fn generate_enum_type(
    type_name: &Ident,
    variants: &[Variant],
    visibility: &syn::Visibility,
    derive: TokenStream2,
    associated_type: TokenStream2,
    ty_generics: &Generics,
    where_clause: Option<&syn::WhereClause>,
    generate_copy: bool,
) -> TokenStream2 {
    let variant_defs = generate_enum_variants(variants, associated_type);
    let copy = if generate_copy {
        let (impl_generics, type_generics, where_clause) = ty_generics.split_for_impl();
        quote! {
            impl #impl_generics Copy for #type_name #type_generics #where_clause {}
        }
    } else {
        quote! {}
    };
    quote! {
        #derive
        #visibility enum #type_name #ty_generics #where_clause {
            #variant_defs
        }
        #copy
    }
}

/// Generate field type definitions
fn generate_field_types(fields: &Fields, associated_type: TokenStream2) -> TokenStream2 {
    match fields {
        Fields::Named(named_fields) => {
            let defs = named_fields.named.iter().map(|f| {
                let name = f.ident.as_ref().unwrap();
                let ty = &f.ty;
                quote! { pub #name: <#ty as peregrine::Data<'h>>::#associated_type }
            });
            quote! { #(#defs),* }
        }
        Fields::Unnamed(unnamed_fields) => {
            let defs = unnamed_fields.unnamed.iter().map(|f| {
                let ty = &f.ty;
                quote! { <#ty as peregrine::Data<'h>>::#associated_type }
            });
            quote! { #(#defs),* }
        }
        Fields::Unit => quote! {},
    }
}

/// Generate enum variant definitions
fn generate_enum_variants(variants: &[Variant], associated_type: TokenStream2) -> TokenStream2 {
    let variant_defs: Vec<_> = variants.iter().map(|variant| {
        let variant_name = &variant.ident;
        match &variant.fields {
            Fields::Named(named_fields) => {
                let field_defs: Vec<_> = named_fields.named.iter().map(|field| {
                    let field_name = field.ident.as_ref().expect("Named field should have an identifier");
                    let field_type = &field.ty;
                    quote! { #field_name: <#field_type as peregrine::Data<'h>>::#associated_type }
                }).collect();
                quote! { #variant_name { #(#field_defs),* } }
            }
            Fields::Unnamed(unnamed_fields) => {
                let field_defs: Vec<_> = unnamed_fields.unnamed.iter().map(|field| {
                    let field_type = &field.ty;
                    quote! { <#field_type as peregrine::Data<'h>>::#associated_type }
                }).collect();
                quote! { #variant_name(#(#field_defs),*) }
            }
            Fields::Unit => quote! { #variant_name },
        }
    }).collect();
    quote! { #(#variant_defs),* }
}

/// Generate Data trait methods
fn generate_data_methods(
    name: &Ident,
    fields: &Fields,
    variants: &[Variant],
    read_type_name: &Ident,
    sample_body: TokenStream2,
    is_struct: bool,
) -> TokenStream2 {
    let (to_read_body, from_read_body) = if is_struct {
        (
            generate_struct_field_operations(
                fields,
                read_type_name,
                |field_name, _field_type| quote! { #field_name: self.#field_name.to_read(written) },
                |field_index, _field_type| quote! { self.#field_index.to_read(written) },
            ),
            generate_struct_field_operations(
                fields,
                name,
                |field_name, field_type| quote! { #field_name: <#field_type as peregrine::Data<'h>>::from_read(read.#field_name, now) },
                |field_index, field_type| quote! { <#field_type as peregrine::Data<'h>>::from_read(read.#field_index, now) },
            ),
        )
    } else {
        (
            generate_enum_operations(
                name,
                variants,
                read_type_name,
                quote! { self },
                |field_name, _field_type| quote! { #field_name: #field_name.to_read(written) },
                |field_name, _field_type| quote! { #field_name.to_read(written) },
            ),
            generate_enum_operations(
                read_type_name,
                variants,
                name,
                quote! { read },
                |field_name, field_type| quote! { #field_name: <#field_type as peregrine::Data<'h>>::from_read(#field_name, now) },
                |field_name, field_type| quote! { <#field_type as peregrine::Data<'h>>::from_read(#field_name, now) },
            ),
        )
    };
    quote! {
        fn to_read(&self, written: peregrine::Time) -> Self::Read { #to_read_body }
        fn from_read(read: Self::Read, now: peregrine::Time) -> Self { #from_read_body }
        fn sample(read: Self::Read, now: peregrine::Time) -> Self::Sample { #sample_body }
    }
}

fn generate_struct_field_operations(
    fields: &Fields,
    type_name: &Ident,
    named_field_op: impl Fn(&Ident, &syn::Type) -> TokenStream2,
    unnamed_field_op: impl Fn(&syn::Index, &syn::Type) -> TokenStream2,
) -> TokenStream2 {
    match fields {
        Fields::Named(named_fields) => {
            let field_calls: Vec<_> = named_fields
                .named
                .iter()
                .map(|field| {
                    let field_name = field
                        .ident
                        .as_ref()
                        .expect("Named field should have an identifier");
                    let field_type = &field.ty;
                    named_field_op(field_name, field_type)
                })
                .collect();
            quote! { #type_name { #(#field_calls),* } }
        }
        Fields::Unnamed(unnamed_fields) => {
            let field_calls: Vec<_> = unnamed_fields
                .unnamed
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    let field_index = syn::Index::from(i);
                    let field_type = &field.ty;
                    unnamed_field_op(&field_index, field_type)
                })
                .collect();
            quote! { #type_name(#(#field_calls),*) }
        }
        Fields::Unit => quote! { #type_name },
    }
}

fn generate_enum_operations(
    source_name: &Ident,
    variants: &[Variant],
    target_name: &Ident,
    match_expr: TokenStream2,
    named_field_op: impl Fn(&Ident, &syn::Type) -> TokenStream2,
    unnamed_field_op: impl Fn(&Ident, &syn::Type) -> TokenStream2,
) -> TokenStream2 {
    let match_arms: Vec<_> = variants.iter().map(|variant| {
        let variant_name = &variant.ident;
        match &variant.fields {
            Fields::Named(named_fields) => {
                let field_patterns: Vec<_> = named_fields.named.iter().map(|field| {
                    let field_name = field.ident.as_ref().expect("Named field should have an identifier");
                    quote! { #field_name }
                }).collect();
                let field_calls: Vec<_> = named_fields.named.iter().map(|field| {
                    let field_name = field.ident.as_ref().expect("Named field should have an identifier");
                    let field_type = &field.ty;
                    named_field_op(field_name, field_type)
                }).collect();
                quote! {
                    #source_name::#variant_name { #(#field_patterns),* } => #target_name::#variant_name { #(#field_calls),* }
                }
            }
            Fields::Unnamed(unnamed_fields) => {
                let field_patterns: Vec<_> = unnamed_fields.unnamed.iter().enumerate().map(|(i, _)| {
                    let field_ident = format_ident!("field_{}", i);
                    quote! { #field_ident }
                }).collect();
                let field_calls: Vec<_> = unnamed_fields.unnamed.iter().enumerate().map(|(i, field)| {
                    let field_ident = format_ident!("field_{}", i);
                    let field_type = &field.ty;
                    unnamed_field_op(&field_ident, field_type)
                }).collect();
                quote! {
                    #source_name::#variant_name(#(#field_patterns),*) => #target_name::#variant_name(#(#field_calls),*)
                }
            }
            Fields::Unit => quote! { #source_name::#variant_name => #target_name::#variant_name },
        }
    }).collect();
    quote! { match #match_expr { #(#match_arms),* } }
}
