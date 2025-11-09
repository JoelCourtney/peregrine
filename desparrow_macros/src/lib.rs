mod op;

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
