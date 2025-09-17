use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Expr, Path};

struct NodeInput {
    processed_code: Expr,
    input_expressions: Vec<Expr>,
}

impl NodeInput {
    pub fn from_expr(input: Expr) -> syn::Result<Self> {
        let mut input_expressions = Vec::new();
        let mut output = input.clone();
        replace_i_invocations(&mut output, &mut input_expressions)?;

        Ok(NodeInput {
            processed_code: output,
            input_expressions,
        })
    }
}

pub fn process_node(input_expr: Expr) -> TokenStream {
    match NodeInput::from_expr(input_expr) {
        Ok(node_input) => {
            let processed = &node_input.processed_code;
            let inputs = &node_input.input_expressions;

            // Generate variable declarations for each input
            let mut input_declarations = Vec::new();
            let mut join_expr = None;
            let mut join_destructure = None;
            for (index, input_expr) in inputs.iter().enumerate() {
                let var_name = format_ident!("peregrine_internal_node_result_{index}");
                let input_name = format_ident!("peregrine_internal_node_input_{index}");
                input_declarations.push(quote! {
                    let #input_name = (#input_expr).into_node();
                });
                if let Some(e) = join_expr {
                    join_expr = Some(quote! {
                        w.join(
                            |w| #input_name.run(w).track(g),
                            |w| #e,
                        )
                    })
                } else {
                    join_expr = Some(quote! {#input_name.run(w).track(g)});
                }
                if let Some(d) = join_destructure {
                    join_destructure = Some(quote! {
                        (#var_name, #d)
                    })
                } else {
                    join_destructure = Some(quote! {#var_name});
                }
            }

            let expanded = quote! {
                {
                    use peregrine::{Node, IntoNode};

                    #(#input_declarations)*
                    peregrine::node::CachedFnWrapper::new(move |w: &peregrine::macro_prelude::Worker, g: peregrine::cache::InvalidatorGenerator<_>| {
                        let #join_destructure = #join_expr;
                        #processed
                    })
                }
            };

            TokenStream::from(expanded)
        }
        Err(err) => TokenStream::from(err.to_compile_error()),
    }
}

fn replace_i_invocations(expr: &mut Expr, collected_inputs: &mut Vec<Expr>) -> syn::Result<()> {
    use syn::{ExprMacro, Macro};

    match expr {
        Expr::Macro(ExprMacro {
            mac: Macro { path, tokens, .. },
            ..
        }) => {
            if is_i_macro(path) {
                // Parse the tokens inside i!() as an expression
                let input_expr: Expr = syn::parse2(tokens.clone())?;
                collected_inputs.push(input_expr);

                // Replace with variable reference
                let input_index = collected_inputs.len() - 1;
                let var_name = format!("peregrine_internal_node_result_{input_index}");
                let var_ident: syn::Ident = syn::parse_str(&var_name)?;
                *expr = syn::parse_quote!(#var_ident);
            }
        }

        // Recursively process other expression types
        Expr::Block(expr_block) => {
            for stmt in &mut expr_block.block.stmts {
                match stmt {
                    syn::Stmt::Expr(e, _) => {
                        replace_i_invocations(e, collected_inputs)?;
                    }
                    syn::Stmt::Local(local) => {
                        if let Some(local_init) = &mut local.init {
                            replace_i_invocations(&mut local_init.expr, collected_inputs)?;
                        }
                    }
                    _ => {}
                }
            }
        }

        Expr::Call(call) => {
            call.args
                .iter_mut()
                .try_for_each(|arg| replace_i_invocations(arg, collected_inputs))?;
        }

        Expr::Binary(binary) => {
            replace_i_invocations(&mut binary.left, collected_inputs)?;
            replace_i_invocations(&mut binary.right, collected_inputs)?;
        }

        Expr::Unary(unary) => {
            replace_i_invocations(&mut unary.expr, collected_inputs)?;
        }

        Expr::Paren(paren) => {
            replace_i_invocations(&mut paren.expr, collected_inputs)?;
        }

        // For other expression types, return as-is for now
        // We can extend this as needed
        _ => {}
    }

    Ok(())
}

fn is_i_macro(path: &Path) -> bool {
    path.segments.len() == 1 && path.segments[0].ident == "i"
}
