use crate::activity::{Activity, StmtOrOp};
use proc_macro2::TokenStream;
use quote::{ToTokens, TokenStreamExt, quote};

impl ToTokens for Activity {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Activity { name, lines } = &self;

        let mut reads = vec![];
        let mut writes = vec![];
        let mut read_writes = vec![];
        for line in &self.lines {
            if let StmtOrOp::Op(op) = line {
                reads.extend(op.reads.clone());
                writes.extend(op.writes.clone());
                read_writes.extend(op.read_writes.clone());
            }
        }

        let resources_used = reads
            .iter()
            .chain(writes.iter())
            .chain(read_writes.iter())
            .collect::<Vec<_>>();

        let timelines_bound = quote! {
            M::Timelines: 'o + #(peregrine::timeline::HasTimeline<'o, #resources_used, M>)+*
        };

        let num_operations = lines.iter().filter(|l| l.is_op()).count();

        let op_functions = lines
            .iter()
            .filter_map(|l| l.get_op())
            .map(|o| o.body_function())
            .collect::<Vec<_>>();

        let result = quote! {
            impl<'o, M: peregrine::Model<'o> + 'o> peregrine::Activity<'o, M> for #name
            where #timelines_bound {
                fn decompose(&'o self, start: peregrine::Time, timelines: &M::Timelines, bump: &'o peregrine::exec::SyncBump) -> peregrine::Result<(peregrine::Duration, Vec<&'o dyn peregrine::operation::Operation<'o, M>>)> {
                    let mut operations: Vec<&'o dyn peregrine::operation::Operation<'o, M>> = Vec::with_capacity(#num_operations);
                    let duration = { #(#lines)* };
                    Ok((duration, operations))
                }
            }

            impl peregrine::ActivityLabel for #name {
                fn label(&self) -> &'static str {
                    peregrine::reexports::peregrine_macros::code_to_str!(#name)
                }
            }

            impl #name {
                #(#op_functions)*
            }
        };

        tokens.append_all(result);
    }
}

impl ToTokens for StmtOrOp {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            StmtOrOp::Stmt(s) => {
                s.to_tokens(tokens);
            }
            StmtOrOp::Op(op) => {
                op.to_tokens(tokens);
            }
        }
    }
}
