mod op;
mod tuple;

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

#[proc_macro]
pub fn impl_op_for_tuple_wrapper(input: TokenStream) -> TokenStream {
    // Parse comma-separated list of type identifiers
    let type_params = syn::parse_macro_input!(input with syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated);

    let type_list: Vec<_> = type_params.into_iter().collect();

    if type_list.len() < 2 {
        panic!("Expected at least two type parameters");
    }

    tuple::generate_op_impl(&type_list)
}
