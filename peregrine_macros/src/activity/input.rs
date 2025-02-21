use std::collections::HashMap;

use crate::activity::{Activity, Op, StmtOrOp};
use proc_macro2::Ident;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{parenthesized, Block, Error, Expr, Path, Result, Stmt, Token};

impl Parse for Activity {
    fn parse(input: ParseStream) -> Result<Self> {
        <Token![for]>::parse(input)?;

        let name: Ident = input.parse()?;

        let mut lines: Vec<StmtOrOp> = vec![];
        while !input.is_empty() {
            lines.push(input.parse()?);
        }

        Ok(Activity { name, lines })
    }
}

impl Parse for StmtOrOp {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Token![@]) {
            Ok(StmtOrOp::Op(input.parse()?))
        } else {
            let forked = input.fork();
            let stmt: Result<Stmt> = forked.parse();
            if stmt.is_ok() {
                Ok(StmtOrOp::Stmt(input.parse()?))
            } else {
                let expr: Expr = input.parse()?;
                Ok(StmtOrOp::Stmt(Stmt::Expr(expr, None)))
            }
        }
    }
}

fn check_paths<'a>(
    iter: impl Iterator<Item = (&'a Ident, &'a Path)>,
    variable: &Ident,
    path: &Path,
) -> Result<()> {
    #[allow(clippy::manual_try_fold)]
    iter.filter(|(v, p)| *p == path && *v != variable)
        .fold(Ok(()), |acc, _| {
            let new_error = Error::new(
                variable.span().join(path.span()).unwrap(),
                "Resource already declared, but with a different variable identifier.",
            );
            match acc {
                Ok(_) => Err(new_error),
                Err(mut e) => {
                    e.combine(new_error);
                    Err(e)
                }
            }
        })
}

impl Parse for Op {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        <Token![@]>::parse(input)?;

        let when_buffer;
        parenthesized!(when_buffer in input);

        let when: Expr = when_buffer.parse()?;

        let mut reads_temp = vec![];
        let mut writes_temp = vec![];

        loop {
            let variable: Ident = input.parse()?;
            <Token![:]>::parse(input)?;
            let path: Path = input.parse()?;

            if reads_temp.contains_key(&variable) {
                return Err(Error::new_spanned(
                    variable,
                    "Identifier declared as read multiple times.",
                ));
            }

            reads_temp.insert(variable, path);

            if input.peek(Token![,]) {
                <Token![,]>::parse(input)?;
            } else {
                <Token![->]>::parse(input)?;
                break;
            }
        }

        loop {
            let variable: Ident = input.parse()?;

            if writes_temp.contains_key(&variable) {
                return Err(Error::new_spanned(
                    variable,
                    "Identifier declared as write multiple times.",
                ));
            }

            if input.peek(Token![:]) {
                <Token![:]>::parse(input)?;
                let path: Path = input.parse()?;
                writes_temp.insert(variable, path);
            } else {
                match reads_temp.get(&variable) {
                    Some(p) => { writes_temp.insert(variable, p.clone()); }
                    None => return Err(Error::new_spanned(variable, "Write identifier declared without resource; same identifier was not found in reads list."))
                }
            }

            if input.peek(Token![,]) {
                <Token![,]>::parse(input)?;
            } else {
                break;
            }
        }

        let mut reads = HashMap::new();
        let mut writes = HashMap::new();
        let mut read_writes = HashMap::new();

        for (v, p) in &reads_temp {
            check_paths(reads_temp.iter().chain(writes_temp.iter()), v, p)?;
            if writes_temp.contains_key(v) {
                read_writes.insert(v.clone(), p.clone());
            } else {
                reads.insert(v.clone(), p.clone());
            }
        }

        for (v, p) in &writes_temp {
            check_paths(writes_temp.iter(), v, p)?;
            if !reads_temp.contains_key(v) {
                writes.insert(v.clone(), p.clone());
            }
        }

        let body: Block = input.parse()?;

        Ok(Op {
            activity: None,
            reads,
            writes,
            read_writes,
            when,
            body,
        })
    }
}
