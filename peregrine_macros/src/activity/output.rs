use crate::activity::{Activity, Invocation, Placement, StmtOrInvoke, Target};
use proc_macro2::TokenStream;
use quote::{ToTokens, TokenStreamExt, quote};

impl ToTokens for Activity {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Activity { path, lines, .. } = &self;

        let mut op_functions = vec![];
        for line in &self.lines {
            if let StmtOrInvoke::Invoke(Invocation {
                target: Target::Inline(op),
                ..
            }) = line
            {
                op_functions.push(op.body_function());
            }
        }

        let num_operations = lines.iter().filter(|l| l.is_invoke()).count();

        let result = quote! {
            impl<'o, M: peregrine::Model<'o>> peregrine::activity::Activity<'o, M> for #path {
                fn decompose(
                    &'o mut self,
                    start: peregrine::activity::Placement<'o, M>,
                    bump: peregrine::reexports::bumpalo_herd::Member<'o>
                ) -> peregrine::Result<(peregrine::Duration, Vec<&'o dyn peregrine::operation::Node<'o, M>>)> {
                    use peregrine::macro_prelude::Node;
                    let mut operations: Vec<&'o dyn Node<'o, M>> = Vec::with_capacity(#num_operations);
                    let duration: peregrine::Result<_> = { #(#lines)* };
                    Ok((duration?, operations))
                }
            }

            impl peregrine::activity::StaticActivity for #path {
                const LABEL: &'static str = peregrine::reexports::peregrine_macros::code_to_str!(#path);
            }

            impl #path {
                #(#op_functions)*
            }
        };

        tokens.append_all(result);
    }
}

impl ToTokens for StmtOrInvoke {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            StmtOrInvoke::Stmt(s) => {
                s.to_tokens(tokens);
            }
            StmtOrInvoke::Invoke(op) => {
                op.to_tokens(tokens);
            }
        }
    }
}

impl ToTokens for Invocation {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let placement = &self.time;
        let op = &self.target;
        let result = match self.target {
            Target::Inline(_) => quote! {
                operations.push((#op)(
                    match #placement {
                        peregrine::activity::Placement::Static(t) => t,
                        _ => todo!()
                    },
                    self,
                    &bump
                ));
            },
            _ => quote! {
                operations.extend((#op)(#placement, self, &bump)?);
            },
        };
        tokens.extend(result);
    }
}

impl ToTokens for Placement {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        if let Some(_delay) = &self.delay {
            todo!()
        }

        let start = &self.start;

        let result = quote! {
            #start
        };

        tokens.extend(result);
    }
}

impl ToTokens for Target {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match &self {
            Target::Inline(op) => op.to_tokens(tokens),
            Target::Activity(expr) | Target::Routine(expr) => {
                let result = quote! {
                    |start, bump| {
                        let output = (#expr).decompose(start, bump)?;
                        Ok::<Vec<&dyn peregrine::operation::Node<'o, M>>, peregrine::Error>(output.1)
                    }
                };
                tokens.extend(result);
            }
        }
    }
}
