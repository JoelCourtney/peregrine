use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Expr, Ident, Pat, Path};

struct OpInput {
    processed_code: Expr,
    inputs: Vec<(Ident, Expr)>,
}

impl OpInput {
    pub fn from_expr(input: Expr) -> syn::Result<Self> {
        let mut input_expressions = Vec::new();
        let mut output = input.clone();
        collect_inputs(&mut output, &mut input_expressions, vec![])?;

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
            let mut input_declarations = Vec::new();
            let mut join_expr = None;
            let mut join_destructure = None;
            let mut input_names = vec![];
            for (index, (var_name, input_expr)) in inputs.iter().enumerate() {
                let input_name = format_ident!("peregrine_internal_op_input_{index}");
                input_names.push(input_name.clone());
                input_declarations.push(quote! {
                    let #input_name = (#input_expr).into_run();
                });
                if let Some(e) = join_expr {
                    join_expr = Some(quote! {
                        worker.join(
                            |worker| #input_name.run(peregrine::Ctx { worker }).track(g),
                            |worker| #e,
                        )
                    })
                } else {
                    join_expr = Some(quote! {#input_name.run(peregrine::Ctx { worker }).track(g)});
                }
                if let Some(d) = join_destructure {
                    join_destructure = Some(quote! {
                        (#var_name, #d)
                    })
                } else {
                    join_destructure = Some(quote! {#var_name});
                }
            }

            let join_statement = if input_declarations.is_empty() {
                quote! {}
            } else {
                quote! {
                    let #join_destructure = {
                        let peregrine::Ctx { worker } = ctx;
                        #join_expr
                    };
                }
            };

            let expanded = quote! {
                {
                    use peregrine::{Run, IntoRun};

                    #(#input_declarations)*
                    peregrine::graph::op::Op::new(
                        move |ctx: peregrine::Ctx, g: peregrine::cache::InvalidatorGenerator<_>| {
                            #join_statement
                            #processed
                        }
                    )
                }
            };

            TokenStream::from(expanded)
        }
        Err(err) => TokenStream::from(err.to_compile_error()),
    }
}

fn collect_inputs(
    expr: &mut Expr,
    collected_inputs: &mut Vec<(Ident, Expr)>,
    mut known_idents: Vec<syn::Ident>,
) -> syn::Result<()> {
    use syn::{ExprMacro, Macro};

    match expr {
        Expr::Macro(ExprMacro {
            mac: Macro { path, tokens, .. },
            ..
        }) => {
            if is_i_macro(path) {
                let input_index = collected_inputs.len();
                let var_ident = format_ident!("peregrine_internal_op_result_{input_index}");

                // Parse the tokens inside i!() as an expression
                let input_expr: Expr = syn::parse2(tokens.clone())?;
                collected_inputs.push((var_ident.clone(), input_expr));

                // Replace with variable reference
                *expr = syn::parse_quote!(#var_ident);
            }
        }

        // Recursively process other expression types
        Expr::Block(expr_block) => {
            for stmt in &mut expr_block.block.stmts {
                match stmt {
                    syn::Stmt::Expr(e, _) => {
                        collect_inputs(e, collected_inputs, known_idents.clone())?;
                    }
                    syn::Stmt::Local(local) => {
                        known_idents.extend(get_idents_from_pattern(&local.pat));
                        if let Some(local_init) = &mut local.init {
                            collect_inputs(
                                &mut local_init.expr,
                                collected_inputs,
                                known_idents.clone(),
                            )?;
                        }
                    }
                    _ => {}
                }
            }
        }

        Expr::Call(call) => {
            call.args
                .iter_mut()
                .try_for_each(|arg| collect_inputs(arg, collected_inputs, known_idents.clone()))?;
        }

        Expr::Binary(binary) => {
            collect_inputs(&mut binary.left, collected_inputs, known_idents.clone())?;
            collect_inputs(&mut binary.right, collected_inputs, known_idents)?;
        }

        Expr::Unary(unary) => {
            collect_inputs(&mut unary.expr, collected_inputs, known_idents)?;
        }

        Expr::Paren(paren) => {
            collect_inputs(&mut paren.expr, collected_inputs, known_idents)?;
        }

        Expr::Group(g) => {
            collect_inputs(&mut g.expr, collected_inputs, known_idents)?;
        }

        Expr::Lit(_) => {}

        Expr::Path(path) => {
            if let Some(ident) = path.path.get_ident()
                && !known_idents.contains(ident)
            {
                collected_inputs.push((ident.clone(), Expr::Path(path.clone())));
            }
        }

        // For other expression types, return as-is for now
        // We can extend this as needed
        e => panic!("Unsupported expression type: {e:?}"),
    }

    Ok(())
}

fn get_idents_from_pattern(pat: &Pat) -> Vec<syn::Ident> {
    match pat {
        Pat::Ident(ident) => vec![ident.ident.clone()],
        Pat::Or(or) => get_idents_from_pattern(or.cases.first().unwrap()),
        Pat::Paren(paren) => get_idents_from_pattern(&paren.pat),
        Pat::Reference(reference) => get_idents_from_pattern(&reference.pat),
        Pat::Slice(slice) => slice
            .elems
            .iter()
            .flat_map(get_idents_from_pattern)
            .collect(),
        Pat::Struct(struct_pat) => struct_pat
            .fields
            .iter()
            .flat_map(|field| get_idents_from_pattern(&field.pat))
            .collect(),
        Pat::Tuple(tuple) => tuple
            .elems
            .iter()
            .flat_map(get_idents_from_pattern)
            .collect(),
        Pat::TupleStruct(tuple_struct) => tuple_struct
            .elems
            .iter()
            .flat_map(get_idents_from_pattern)
            .collect(),
        Pat::Type(type_pat) => get_idents_from_pattern(&type_pat.pat),
        _ => vec![],
    }
}

fn is_i_macro(path: &Path) -> bool {
    path.segments.len() == 1 && path.segments[0].ident == "i"
}
