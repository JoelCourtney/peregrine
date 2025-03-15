use syn::parse_macro_input;

use crate::model::Model;
use crate::operation::Op;
use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use rand::Rng;

mod model;
mod operation;

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
