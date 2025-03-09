use crate::resource::{HistoryType, Resource};
use quote::ToTokens;
use syn::parse::discouraged::Speculative;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, ExprLit, Ident, Lit, LitBool, Token};

impl Parse for Resource {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let visibility = input.parse()?;

        let mut history = if input.peek(Token![ref]) {
            let _: Token![ref] = input.parse()?;
            HistoryType::Deref
        } else {
            HistoryType::Copy
        };

        let name = input.parse()?;

        let _: Token![:] = input.parse()?;

        let write_type = input.parse()?;

        let mut dynamic = false;
        while input.peek(Token![;]) {
            let _: Token![;] = input.parse()?;

            if input.peek(Ident) {
                let ident: Ident = input.parse()?;
                let _: Token![=] = input.parse()?;
                let text = ident.to_string();

                let forked = input.fork();
                let value: Expr = forked.parse()?;

                match text.trim() {
                    "dynamic" => {
                        dynamic = match value {
                            Expr::Lit(ExprLit { lit: Lit::Bool(LitBool { value, .. }), .. }) => {
                                value
                            }
                            _ => return Err(input.error("Unexpected value for `dynamic`. Expected either true or false."))
                        }
                    }
                    "history" => {
                        let value_text = value.to_token_stream().to_string();
                        history = match value_text.trim() {
                            "copy" => {
                                if history == HistoryType::Deref {
                                    return Err(input.error("Conflicting history type. History was already declared as deref with the `ref` keyword."))
                                }
                                HistoryType::Copy
                            },
                            "deref" => HistoryType::Deref,
                            _ => return Err(input.error("Unexpected history type. Expected `copy` or `deref` (without quotes)."))
                        }
                    }
                    _ => return Err(input.error("Unexpected resource property. Expected `dynamic` or `history` (without quotes)."))
                }

                input.advance_to(&forked);
            }
        }

        Ok(Resource {
            visibility,
            name,
            write_type,
            dynamic,
            history,
        })
    }
}
