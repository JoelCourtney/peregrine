use quote::{ToTokens, format_ident, quote};
use rand::Rng;
use syn::{DeriveInput, LitInt, parse_macro_input};

use crate::maybe_hash::{generate_enum_impl, generate_struct_impl};
use crate::model::Model;
use crate::node::Node;
use crate::operation::Op;
use crate::resource::MultiResource;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;

mod data;
mod maybe_hash;
mod model;
mod node;
mod operation;
mod resource;

#[cfg(feature = "pregenerated")]
const MAX_PREGENERATED_ORDER: i32 = 5;

#[cfg(not(feature = "pregenerated"))]
const MAX_PREGENERATED_ORDER: i32 = -1;

#[proc_macro]
pub fn op(input: TokenStream) -> TokenStream {
    let op = parse_macro_input!(input as Op);
    op.into_token_stream().into()
}

#[proc_macro]
pub fn internal_op(input: TokenStream) -> TokenStream {
    let mut op = parse_macro_input!(input as Op);
    op.internal = true;
    op.into_token_stream().into()
}

#[proc_macro]
pub fn model(input: TokenStream) -> TokenStream {
    let model = parse_macro_input!(input as Model);
    model.into_token_stream().into()
}

#[proc_macro]
pub fn resource(input: TokenStream) -> TokenStream {
    let resources = parse_macro_input!(input as MultiResource);
    resources.into_token_stream().into()
}

#[proc_macro]
pub fn code_to_str(input: TokenStream) -> TokenStream {
    let string = input.to_string();
    let trimmed = string.trim();
    quote! { #trimmed }.into()
}

#[proc_macro]
pub fn random_u64(_input: TokenStream) -> TokenStream {
    let num = rand::rng().random::<u64>();
    quote! { #num }.into()
}

#[proc_macro]
pub fn impl_read_structs(input: TokenStream) -> TokenStream {
    let num_reads = parse_macro_input!(input as LitInt)
        .base10_parse::<usize>()
        .unwrap();

    impl_read_structs_internal(num_reads).into()
}

fn impl_read_structs_internal(num_reads: usize) -> TokenStream2 {
    let name = format_ident!("Reads{num_reads}");
    let read_upstreams = (0..num_reads)
        .map(|i| format_ident!("upstream_{i}"))
        .collect::<Vec<_>>();
    let read_responses = (0..num_reads)
        .map(|i| format_ident!("upstream_response_{i}"))
        .collect::<Vec<_>>();
    let read_types = (0..num_reads)
        .map(|i| format_ident!("Read{i}"))
        .collect::<Vec<_>>();

    quote! {
        struct #name<'o, #(#read_types: peregrine::Resource,)*> {
            #(#read_upstreams: Option<&'o dyn peregrine::internal::macro_prelude::Upstream<'o, #read_types>>,)*
            #(#read_responses: Option<peregrine::internal::macro_prelude::InternalResult<(u64, <<#read_types as peregrine::Resource>::Data as peregrine::Data<'o>>::Read)>>,)*
            lifetime: std::marker::PhantomData<&'o ()>
        }

        impl<'o, #(#read_types: peregrine::Resource,)*> Default for #name<'o, #(#read_types,)*> {
            fn default() -> Self {
                Self {
                    #(#read_upstreams: None,)*
                    #(#read_responses: None,)*
                    lifetime: std::marker::PhantomData
                }
            }
        }
    }
}

#[proc_macro]
pub fn impl_write_structs(input: TokenStream) -> TokenStream {
    let num_writes = parse_macro_input!(input as LitInt)
        .base10_parse::<usize>()
        .unwrap();

    impl_write_structs_internal(num_writes).into()
}

fn impl_write_structs_internal(num_writes: usize) -> TokenStream2 {
    let writes_name = format_ident!("Writes{num_writes}");
    let continuations_name = format_ident!("Continuations{num_writes}");
    let downstreams_name = format_ident!("Downstreams{num_writes}");

    let writes = (0..num_writes)
        .map(|i| format_ident!("write_{i}"))
        .collect::<Vec<_>>();
    let write_types = (0..num_writes)
        .map(|i| format_ident!("Write{i}"))
        .collect::<Vec<_>>();

    quote! {
        #[derive(Copy, Clone)]
        struct #writes_name<'o, #(#write_types: peregrine::Resource,)*> {
            #(#writes: <<#write_types as peregrine::Resource>::Data as peregrine::Data<'o>>::Read,)*
        }

        #[allow(non_camel_case_types)]
        enum #continuations_name<'o, #(#write_types: peregrine::Resource,)*> {
            #(#writes(peregrine::internal::macro_prelude::Continuation<'o, #write_types>),)*
        }

        #[allow(non_camel_case_types)]
        enum #downstreams_name<'o, #(#write_types: peregrine::Resource,)*> {
            #(#writes(&'o dyn peregrine::internal::macro_prelude::Downstream<'o, #write_types>),)*
        }
    }
}

#[proc_macro]
pub fn impl_nodes(input: TokenStream) -> TokenStream {
    let order = parse_macro_input!(input as LitInt)
        .base10_parse::<usize>()
        .unwrap();
    let mut result = quote! {};
    for num_read_onlys in 0..=order {
        for num_read_writes in 0..=order {
            for num_write_onlys in 0..=order {
                if num_write_onlys + num_read_writes > 0 {
                    let node = impl_node(num_read_onlys, num_read_writes, num_write_onlys);
                    result = quote! {
                        #result
                        #node
                    }
                }
            }
        }
    }
    quote! {
        use peregrine::*;
        use peregrine::internal::macro_prelude::*;
        #result
    }
    .into()
}

fn impl_node(
    num_read_onlys: usize,
    num_read_writes: usize,
    num_write_onlys: usize,
) -> TokenStream2 {
    let num_reads = num_read_writes + num_read_onlys;
    let num_writes = num_read_writes + num_write_onlys;

    let read_only_upstreams = (0..num_read_onlys)
        .map(|i| format_ident!("upstream_{i}"))
        .collect::<Vec<_>>();
    let read_write_upstreams = (num_read_onlys..num_reads)
        .map(|i| format_ident!("upstream_{i}"))
        .collect::<Vec<_>>();
    let read_only_responses = (0..num_read_onlys)
        .map(|i| format_ident!("upstream_response_{i}"))
        .collect::<Vec<_>>();
    let read_write_responses = (num_read_onlys..num_reads)
        .map(|i| format_ident!("upstream_response_{i}"))
        .collect::<Vec<_>>();
    let read_response_hashes = (0..num_reads)
        .map(|i| format_ident!("upstream_response_hash_{i}"))
        .collect::<Vec<_>>();

    let read_only_types = (0..num_read_onlys)
        .map(|i| format_ident!("Read{i}"))
        .collect::<Vec<_>>();
    let write_only_types = (0..num_write_onlys)
        .map(|i| format_ident!("Write{i}"))
        .collect::<Vec<_>>();
    let read_write_types = (0..num_read_writes)
        .map(|i| format_ident!("ReadWrite{i}"))
        .collect::<Vec<_>>();

    let writes = (0..num_writes)
        .map(|i| format_ident!("write_{i}"))
        .collect::<Vec<_>>();

    let node = Node {
        name: format_ident!("NodeR{num_read_onlys}Rw{num_read_writes}W{num_write_onlys}"),
        reads_name: format_ident!("Reads{num_reads}"),
        writes_name: format_ident!("Writes{num_writes}"),
        continuations_name: format_ident!("Continuations{num_writes}"),
        downstreams_name: format_ident!("Downstreams{num_writes}"),
        read_only_upstreams,
        read_write_upstreams,
        read_only_responses,
        read_write_responses,
        read_response_hashes,
        writes,
        read_only_types,
        read_write_types,
        write_only_types,
    };

    node.generate()
}

#[proc_macro]
pub fn delay(input: TokenStream) -> TokenStream {
    use proc_macro2::TokenStream as TokenStream2;
    /// Parses `{ expr => tt }` into (expr, tt)
    use syn::{
        Expr, Result as SynResult, Token,
        parse::{Parse, ParseStream},
    };

    struct DelayInput {
        expr: Expr,
        _arrow: Token![=>],
        tt: TokenStream2,
    }

    impl Parse for DelayInput {
        fn parse(input: ParseStream) -> SynResult<Self> {
            let expr: Expr = input.parse()?;
            let _arrow: Token![=>] = input.parse()?;
            let tt: TokenStream2 = input.parse()?;
            Ok(DelayInput { expr, _arrow, tt })
        }
    }

    let DelayInput { expr, tt, .. } = syn::parse_macro_input!(input as DelayInput);

    let expanded = quote! {
        {
            use peregrine::internal::macro_prelude::{builtins::elapsed, peregrine_grounding};
            move |placement| peregrine::internal::macro_prelude::Delay {
                node: (op! { w: peregrine_grounding = r: elapsed + std::cmp::min(#tt, #expr); })(placement),
                min: placement.min(),
                max: placement.max() + #expr,
            }
        }
    };
    expanded.into()
}

#[proc_macro_derive(Data, attributes(sample))]
pub fn derive_data(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    data::generate_data_impl(input)
}

#[proc_macro_derive(MaybeHash, attributes(hash_if, always_hash))]
pub fn derive_maybe_hash(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Look for #[hash_if = "expr"]
    let mut hash_if_expr = None;
    for attr in &input.attrs {
        if attr.path().is_ident("hash_if") {
            // Parse the attribute as #[hash_if = "expr"]
            if let Ok(syn::Expr::Lit(expr_lit)) = attr.parse_args() {
                if let syn::Lit::Str(litstr) = expr_lit.lit {
                    hash_if_expr =
                        Some(litstr.value().parse().expect("Invalid hash_if expression"));
                }
            }
        }
    }

    let expanded = match &input.data {
        syn::Data::Struct(data) => generate_struct_impl(
            name,
            &data.fields,
            impl_generics,
            ty_generics,
            where_clause,
            hash_if_expr,
        ),
        syn::Data::Enum(data) => generate_enum_impl(
            name,
            &data.variants.iter().collect::<Vec<_>>(),
            impl_generics,
            ty_generics,
            where_clause,
            hash_if_expr,
        ),
        syn::Data::Union(_) => {
            return syn::Error::new_spanned(
                &input.ident,
                "MaybeHash derive macro does not support unions",
            )
            .to_compile_error()
            .into();
        }
    };

    TokenStream::from(expanded)
}
