mod activity;
mod auto_source;
mod op;
mod undo;

use proc_macro::TokenStream;
use quote::quote;

#[proc_macro]
pub fn op(input: TokenStream) -> TokenStream {
    // Wrap the input in braces to make it parseable as a block expression
    let input2: proc_macro2::TokenStream = input.into();
    let wrapped_tokens = quote! { { #input2 } };
    let input_expr = syn::parse2(wrapped_tokens).expect("Failed to parse wrapped input");

    op::process_op(input_expr)
}

#[proc_macro_derive(Undo, attributes(undo))]
pub fn derive_undo(input: TokenStream) -> TokenStream {
    undo::derive_undo(input)
}

#[proc_macro_derive(AutoSource)]
pub fn derive_auto_source(input: TokenStream) -> TokenStream {
    auto_source::derive_auto_source(input)
}

#[proc_macro_attribute]
pub fn activity(attr: TokenStream, item: TokenStream) -> TokenStream {
    activity::activity_attribute(attr, item)
}
