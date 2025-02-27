use proc_macro2::{Ident, TokenStream};
use quote::ToTokens;
use syn::{Block, Expr, Item, Path, Stmt};
use crate::operation::{Context, Op};

mod input;
mod output;

pub fn process_activity(mut activity: Activity) -> TokenStream {
    let path = activity.path.clone();

    for line in &mut activity.lines {
        if let StmtOrInvoke::Invoke(_, Target::Inline(op)) = line {
            op.context = Context::Activity(path.clone());
        }
    }

    activity.into_token_stream()
}

pub struct Activity {
    path: Path,
    structure: ActivityStructure,
    lines: Vec<StmtOrInvoke>,
}

pub enum ActivityStructure {
    Ident(Ident),
    Item(Item),
}

enum StmtOrInvoke {
    Stmt(Stmt),
    Invoke(InvocationTime, Target),
}

struct InvocationTime {
    start: Expr,
    delay: Delay,
}

impl StmtOrInvoke {
    fn is_invoke(&self) -> bool {
        matches!(self, StmtOrInvoke::Invoke(..))
    }
    fn get_invoke(&self) -> Option<(&InvocationTime, &Target)> {
        match self {
            StmtOrInvoke::Stmt(_) => None,
            StmtOrInvoke::Invoke(when, target) => Some((when, target)),
        }
    }
}

enum Delay {
    Expr(Expr),
    Inline(Op)
}

enum Target {
    Inline(Op),
    Activity(Expr),
    Routine(Expr),
}

