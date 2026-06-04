use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

pub fn derive_auto_source(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = &input.ident;
    let generics = &input.generics;

    // Split generics for impl block
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Add Self: Data bound to the where clause
    let where_clause = if let Some(clause) = where_clause {
        quote! { #clause, Self: peregrine::data::Data }
    } else {
        quote! { where Self: peregrine::data::Data }
    };

    let expanded = quote! {
        impl #impl_generics peregrine::Upstream for #name #ty_generics #where_clause {
            type Output = Self;

            #[inline(always)]
            fn request<'s>(&self, ctx: peregrine::Ctx<'_, '_, 's>, callback: peregrine::Callback<'s, Self::Output>)
            where
                Self: 's,
            {
                callback.call(peregrine::cache::Cached::Constant(self.clone()), ctx);
            }
        }
    };

    TokenStream::from(expanded)
}
