use proc_macro2::{Ident, TokenStream};
use quote::ToTokens;
use std::collections::HashMap;
use syn::{Block, Expr, Path, Stmt};

mod input;
mod operation;
mod output;

pub fn process_activity(mut activity: Activity) -> TokenStream {
    let name = activity.name.clone();

    for line in &mut activity.lines {
        if let StmtOrOp::Op(op) = line {
            op.activity = Some(name.clone());
        }
    }

    activity.into_token_stream()
}

pub struct Activity {
    name: Ident,
    lines: Vec<StmtOrOp>,
}

enum StmtOrOp {
    Stmt(Stmt),
    Op(Op),
}

impl StmtOrOp {
    fn is_op(&self) -> bool {
        matches!(self, StmtOrOp::Op(_))
    }
}

struct Op {
    activity: Option<Ident>,
    reads: Vec<Ident>,
    writes: Vec<Ident>,
    read_writes: Vec<Ident>,
    when: Expr,
    body: Block,
}
