use crate::activity::{Activity, ActivityStructure, Invocation, Placement, StmtOrInvoke, Target};
use syn::parse::discouraged::Speculative;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, ItemEnum, ItemStruct, Path, Result, Stmt, Token, braced, parenthesized};

impl Parse for Activity {
    fn parse(input: ParseStream) -> Result<Self> {
        let lookahead = input.lookahead1();
        let (path, structure) = if lookahead.peek(Token![for]) {
            <Token![for]>::parse(input)?;
            let path: Path = input.parse()?;
            (path.clone(), ActivityStructure::Path)
        } else if lookahead.peek(Token![struct]) {
            let item: ItemStruct = input.parse()?;
            let path = Path::from(item.ident.clone());
            (path, ActivityStructure::Item)
        } else if lookahead.peek(Token![enum]) {
            let item: ItemEnum = input.parse()?;
            let path = Path::from(item.ident.clone());
            (path, ActivityStructure::Item)
        } else {
            return Err(lookahead.error());
        };

        let mut lines: Vec<StmtOrInvoke> = vec![];
        while !input.is_empty() {
            lines.push(input.parse()?);
        }

        Ok(Activity {
            path,
            _structure: structure,
            lines,
        })
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

impl Parse for Invocation {
    fn parse(input: ParseStream) -> Result<Self> {
        <Token![@]>::parse(input)?;

        let start_body;
        parenthesized!(start_body in input);
        let start_expr = start_body.parse()?;
        assert!(start_body.is_empty());

        let delay_op = if input.peek(Token![+]) {
            <Token![+]>::parse(input)?;
            let delay_body;
            parenthesized!(delay_body in input);

            Some(delay_body.parse()?)
        } else {
            None
        };

        let target = input.parse()?;

        Ok(Invocation {
            time: Placement {
                start: start_expr,
                delay: delay_op,
            },
            target,
        })
    }
}

impl Parse for Target {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(syn::Ident) {
            let forked = input.fork();
            let ident: syn::Ident = forked.parse()?;
            let is_spawn = if ident == "spawn" {
                input.advance_to(&forked);
                true
            } else {
                false
            };

            let expr: Expr = input.parse()?;
            let _: Token![;] = input.parse()?;

            if is_spawn {
                Ok(Target::Activity(expr))
            } else {
                Ok(Target::Routine(expr))
            }
        } else {
            let op_body;
            braced!(op_body in input);
            Ok(Target::Inline(op_body.parse()?))
        }
    }
}
