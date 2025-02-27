use crate::activity::{Activity, Op, StmtOrInvoke};
use proc_macro2::Ident;
use syn::parse::{Parse, ParseStream};
use syn::{Block, Error, Expr, Result, Stmt, Token, parenthesized};

impl Parse for Activity {
    fn parse(input: ParseStream) -> Result<Self> {
        <Token![for]>::parse(input)?;

        let name: Ident = input.parse()?;

        let mut lines: Vec<StmtOrInvoke> = vec![];
        while !input.is_empty() {
            lines.push(input.parse()?);
        }

        Ok(Activity { name, lines })
    }
}

impl Parse for StmtOrInvoke {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Token![@]) {
            Ok(StmtOrInvoke::Invoke(input.parse()?))
        } else {
            let forked = input.fork();
            let stmt: Result<Stmt> = forked.parse();
            if stmt.is_ok() {
                Ok(StmtOrInvoke::Stmt(input.parse()?))
            } else {
                let expr: Expr = input.parse()?;
                Ok(StmtOrInvoke::Stmt(Stmt::Expr(expr, None)))
            }
        }
    }
}

