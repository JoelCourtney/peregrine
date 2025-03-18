use syn::{LitInt, parse_macro_input};

use crate::model::Model;
use crate::node::Node;
use crate::operation::Op;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, format_ident, quote};
use rand::Rng;

mod model;
mod node;
mod operation;

#[cfg(feature = "pregenerated")]
const MAX_PREGENERATED_ORDER: usize = 5;

#[cfg(not(feature = "pregenerated"))]
const MAX_PREGENERATED_ORDER: usize = 0;

#[proc_macro]
pub fn op(input: TokenStream) -> TokenStream {
    let op = parse_macro_input!(input as Op);
    op.into_token_stream().into()
}

#[proc_macro]
pub fn model(input: TokenStream) -> TokenStream {
    let model = parse_macro_input!(input as Model);
    model.into_token_stream().into()
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
        struct #name<'o, #(#read_types: macro_prelude::Resource,)*> {
            #(#read_upstreams: Option<&'o dyn macro_prelude::Upstream<'o, #read_types>>,)*
            #(#read_responses: Option<macro_prelude::InternalResult<(u64, <<#read_types as macro_prelude::Resource>::Data as macro_prelude::Data<'o>>::Read)>>,)*
            lifetime: std::marker::PhantomData<&'o ()>
        }

        impl<'o, #(#read_types: macro_prelude::Resource,)*> Default for #name<'o, #(#read_types,)*> {
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
        struct #writes_name<'o, #(#write_types: macro_prelude::Resource,)*> {
            #(#writes: <<#write_types as macro_prelude::Resource>::Data as macro_prelude::Data<'o>>::Read,)*
        }

        #[allow(non_camel_case_types)]
        enum #continuations_name<'o, #(#write_types: macro_prelude::Resource,)*> {
            #(#writes(macro_prelude::Continuation<'o, #write_types>),)*
        }

        #[allow(non_camel_case_types)]
        enum #downstreams_name<'o, #(#write_types: macro_prelude::Resource,)*> {
            #(#writes(&'o dyn macro_prelude::Downstream<'o, #write_types>),)*
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
    result.into()
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
