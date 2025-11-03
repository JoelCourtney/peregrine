use proc_macro::TokenStream;
use quote::{format_ident, quote};

pub fn generate_op_impl(types: &[syn::Ident]) -> TokenStream {
    let arity = types.len();

    // Generate generic bounds: A: Run, B: Run, ...
    let generic_bounds = types.iter().map(|t| quote! { #t: Run });

    // Generate output type: (A::Output, B::Output, ...)
    let output_types = types
        .iter()
        .map(|t| quote! { #t::Output })
        .collect::<Vec<_>>();

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
        impl<#(#generic_bounds),*> Run for TupleWrapper<(#(#types,)*), (#(#output_types,)*)> where #(#types::Output: Clone + 'static),* {
            type Output = (#(#output_types),*);

            fn world_id(&self) -> WorldId {
                self.2
            }
            fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
                self.1.resolve(ctx.worker, |g| {
                    let Ctx { world, worker } = ctx;
                    let #destructure_pattern = #run_cache_join_expr;
                    #merge_chain
                }, false)
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
            worker.join(
                |worker| #first.run(Ctx { world, worker }).track(g),
                |worker| #second.run(Ctx { world, worker }).track(g)
            )
        }
    } else {
        // For 3+, nest the joins: first element vs. nested join of the rest
        let first = &field_accesses[0];
        let rest = &field_accesses[1..];
        let nested_run_cache = generate_nested_joins(rest);

        quote! {
            worker.join(
                |worker| #first.run(Ctx { world, worker }).track(g),
                |worker| #nested_run_cache
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
    let mut result = quote! {};
    for i in 0..arity {
        let var = format_ident!("{}", (b'a' + i as u8) as char);
        result = quote! { #result #var, };
    }
    quote! { (#result) }
}
