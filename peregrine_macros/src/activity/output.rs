use crate::activity::operation::process_operation;
use crate::activity::{Activity, Op, StmtOrOp};
use proc_macro2::TokenStream;
use quote::{quote, ToTokens, TokenStreamExt};

impl ToTokens for Activity {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Activity { name, lines } = &self;

        let mut reads = vec![];
        let mut writes = vec![];
        let mut read_writes = vec![];
        for line in &self.lines {
            if let StmtOrOp::Op(op) = line {
                reads.extend(op.reads.values().cloned());
                writes.extend(op.writes.values().cloned());
                read_writes.extend(op.read_writes.values().cloned());
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

        let result = quote! {
            impl<'o, M: peregrine::Model<'o> + 'o> peregrine::Activity<'o, M> for #name
            where #timelines_bound {
                fn decompose(&'o self, start: peregrine::Time, timelines: &M::Timelines, bump: &'o peregrine::exec::SyncBump) -> (peregrine::Duration, Vec<&'o dyn peregrine::operation::Operation<'o, M>>) {
                    let mut operations: Vec<&'o dyn peregrine::operation::Operation<'o, M>> = Vec::with_capacity(#num_operations);
                    let duration = { #(#lines)* };
                    (duration, operations)
                }
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

impl ToTokens for Op {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Op {
            activity,
            reads,
            writes,
            read_writes,
            when,
            body: op,
        } = self;

        let activity = activity.clone().expect("activity name was not set");

        let read_variables = reads.keys().chain(read_writes.keys());
        let read_paths = reads.values().chain(read_writes.values());

        let write_variables = writes.keys().chain(read_writes.keys());
        let write_paths = writes.values().chain(read_writes.values());

        let input = quote! {
            activity #activity;
            reads #(#read_variables: #read_paths),*;
            writes #(#write_variables: #write_paths),*;
            when #when;
            op #op
        };
        let result = process_operation(input.to_string());
        tokens.append_all(result);
    }
}
