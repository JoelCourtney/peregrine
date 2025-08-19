use proc_macro::TokenStream;
use quote::{format_ident, quote};

pub fn generate_node_impl(types: &[syn::Ident]) -> TokenStream {
    let arity = types.len();

    // Generate generic bounds: A: Node, B: Node, ...
    let generic_bounds = types.iter().map(|t| quote! { #t: Node });

    // Generate output type: (A::Output, B::Output, ...)
    let output_types = types.iter().map(|t| quote! { #t::Output });

    // Generate tuple type for the wrapper: (A, B, ...)
    let tuple_type = quote! { (#(#types,)*) };

    // Generate field accesses: self.0.0, self.0.1, ...
    let field_accesses: Vec<_> = (0..arity)
        .map(|i| {
            let index = syn::Index::from(i);
            quote! { self.0.#index }
        })
        .collect();

    // Generate the nested join expression
    let run_cache_join_expr = generate_nested_joins(&field_accesses);

    // Generate destructuring pattern based on arity
    let destructure_pattern = generate_destructure_pattern(arity);

    // Generate merge chain for run_cache
    let merge_chain = generate_merge_chain(arity);

    let expanded = quote! {
        impl<#(#generic_bounds),*> Node for TupleWrapper<#tuple_type> {
            type Output = (#(#output_types),*);

            fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
                let #destructure_pattern = #run_cache_join_expr;
                #merge_chain
            }
        }
    };

    TokenStream::from(expanded)
}

fn generate_nested_joins(field_accesses: &[proc_macro2::TokenStream]) -> proc_macro2::TokenStream {
    let arity = field_accesses.len();

    if arity == 2 {
        let first = &field_accesses[0];
        let second = &field_accesses[1];

        quote! {
            w.join(
                |w| #first.run(w),
                |w| #second.run(w)
            )
        }
    } else {
        // For 3+, nest the joins: first element vs. nested join of the rest
        let first = &field_accesses[0];
        let rest = &field_accesses[1..];
        let nested_run_cache = generate_nested_joins(rest);

        quote! {
            w.join(
                |w| #first.run(w),
                |w| #nested_run_cache
            )
        }
    }
}

fn generate_destructure_pattern(arity: usize) -> proc_macro2::TokenStream {
    if arity == 2 {
        quote! { (a,b) }
    } else {
        // Build nested pattern: (a,(b,(c,d))) for 4-tuple
        // Start with the last two variables as a pair
        let last_var = format_ident!("{}", (b'a' + (arity - 1) as u8) as char);
        let second_last_var = format_ident!("{}", (b'a' + (arity - 2) as u8) as char);
        let mut pattern = quote! { (#second_last_var,#last_var) };

        // Work backwards, wrapping each previous variable
        for i in (1..arity - 2).rev() {
            let var = format_ident!("{}", (b'a' + i as u8) as char);
            pattern = quote! { (#var,#pattern) };
        }

        // Finally wrap with 'a'
        quote! { (a,#pattern) }
    }
}

fn generate_merge_chain(arity: usize) -> proc_macro2::TokenStream {
    if arity == 2 {
        quote! { a.merge(b, |a,b| (a,b)) }
    } else {
        // Start with merging a and b
        let mut result = quote! { a.merge(b, |a,b| (a,b)) };

        // Chain additional merges
        for i in 2..arity {
            let var = format_ident!("{}", (b'a' + i as u8) as char);

            // Generate pattern for previous tuple: (a,b), (a,b,c), etc.
            let prev_vars: Vec<_> = (0..i)
                .map(|j| format_ident!("{}", (b'a' + j as u8) as char))
                .collect();
            let prev_pattern = quote! { (#(#prev_vars),*) };

            // Generate new tuple with added variable
            let new_vars: Vec<_> = (0..=i)
                .map(|j| format_ident!("{}", (b'a' + j as u8) as char))
                .collect();
            let new_tuple = quote! { (#(#new_vars),*) };

            result = quote! { #result.merge(#var, |#prev_pattern, #var| #new_tuple) };
        }

        result
    }
}
