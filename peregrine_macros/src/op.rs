use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Block, Expr, Ident, Path};

struct OpInput {
    processed_code: Expr,
    inputs: Vec<(Ident, Expr)>,
}

impl OpInput {
    pub fn from_expr(input: Expr) -> syn::Result<Self> {
        let mut input_expressions = Vec::new();
        let mut output = input.clone();
        collect_inputs(&mut output, &mut input_expressions)?;

        Ok(OpInput {
            processed_code: output,
            inputs: input_expressions,
        })
    }
}

pub fn process_op(input_expr: Expr) -> TokenStream {
    match OpInput::from_expr(input_expr) {
        Ok(node_input) => {
            let processed = &node_input.processed_code;
            let inputs = &node_input.inputs;

            // Generate variable declarations for each input
            let mut upstreams = vec![];
            let mut input_names = vec![];
            for (input_name, input_expr) in inputs {
                input_names.push(input_name);
                upstreams.push(quote! {
                    #input_expr
                });
            }

            let expanded = quote! {
                {
                    use peregrine::Upstream;

                    let (#(#input_names,)*) = (#(#upstreams,)*);
                    let node_ids = [#(#input_names.node_id(),)*].into_iter().filter_map(|i| i);
                    peregrine::graph::op::Op::new(
                        (#(#input_names,)*),
                        move |(#(#input_names,)*)| {
                            #processed
                        },
                        node_ids
                    )
                }
            };

            TokenStream::from(expanded)
        }
        Err(err) => TokenStream::from(err.to_compile_error()),
    }
}

fn collect_inputs(expr: &mut Expr, collected_inputs: &mut Vec<(Ident, Expr)>) -> syn::Result<()> {
    use syn::{ExprMacro, Macro};

    match expr {
        Expr::Macro(ExprMacro {
            mac: Macro { path, tokens, .. },
            ..
        }) => {
            if is_i_macro(path) {
                let input_index = collected_inputs.len();
                let var_ident = format_ident!("peregrine_internal_op_input_{input_index}");

                // Parse the tokens inside i!() as an expression
                let input_expr: Expr = syn::parse2(tokens.clone())?;
                collected_inputs.push((var_ident.clone(), input_expr));

                // Replace with variable reference
                *expr = syn::parse_quote!(#var_ident);
            }
        }

        // Recursively process other expression types
        Expr::Block(expr_block) => {
            collect_inputs_block(&mut expr_block.block, collected_inputs)?;
        }

        Expr::Call(call) => {
            call.args
                .iter_mut()
                .try_for_each(|arg| collect_inputs(arg, collected_inputs))?;
        }

        Expr::Binary(binary) => {
            collect_inputs(&mut binary.left, collected_inputs)?;
            collect_inputs(&mut binary.right, collected_inputs)?;
        }

        Expr::Unary(unary) => {
            collect_inputs(&mut unary.expr, collected_inputs)?;
        }

        Expr::Paren(paren) => {
            collect_inputs(&mut paren.expr, collected_inputs)?;
        }

        Expr::Group(g) => {
            collect_inputs(&mut g.expr, collected_inputs)?;
        }

        Expr::Lit(_) => {}

        Expr::Path(_) => {}

        Expr::Tuple(tuple) => {
            for elem in &mut tuple.elems {
                collect_inputs(elem, collected_inputs)?;
            }
        }

        Expr::MethodCall(method_call) => {
            for arg in &mut method_call.args {
                collect_inputs(arg, collected_inputs)?;
            }
            collect_inputs(&mut method_call.receiver, collected_inputs)?;
        }

        Expr::Struct(struct_expr) => {
            for field in &mut struct_expr.fields {
                collect_inputs(&mut field.expr, collected_inputs)?;
            }
            if let Some(rest) = &mut struct_expr.rest {
                collect_inputs(&mut *rest, collected_inputs)?;
            }
        }

        Expr::Reference(ref_expr) => {
            collect_inputs(&mut ref_expr.expr, collected_inputs)?;
        }

        Expr::Closure(closure) => {
            collect_inputs(&mut closure.body, collected_inputs)?;
        }

        Expr::Assign(assign) => {
            collect_inputs(&mut assign.right, collected_inputs)?;
        }

        Expr::Let(let_expr) => {
            collect_inputs(&mut let_expr.expr, collected_inputs)?;
        }

        Expr::If(if_expr) => {
            collect_inputs(&mut if_expr.cond, collected_inputs)?;
            collect_inputs_block(&mut if_expr.then_branch, collected_inputs)?;
            if let Some((_, else_branch)) = &mut if_expr.else_branch {
                collect_inputs(&mut *else_branch, collected_inputs)?;
            }
        }

        // For other expression types, return as-is for now
        // We can extend this as needed
        e => todo!("Unsupported expression type: {e:?}"),
    }

    Ok(())
}

fn collect_inputs_block(
    block: &mut Block,
    collected_inputs: &mut Vec<(Ident, Expr)>,
) -> syn::Result<()> {
    for stmt in &mut block.stmts {
        match stmt {
            syn::Stmt::Expr(e, _) => {
                collect_inputs(e, collected_inputs)?;
            }
            syn::Stmt::Local(local) => {
                if let Some(local_init) = &mut local.init {
                    collect_inputs(&mut local_init.expr, collected_inputs)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn is_i_macro(path: &Path) -> bool {
    path.segments.len() == 1 && path.segments[0].ident == "i"
}
