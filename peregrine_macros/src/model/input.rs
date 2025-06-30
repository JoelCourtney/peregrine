use crate::model::{Daemon, Model};
use proc_macro2::Ident;
use syn::parse::{Parse, ParseStream};
use syn::{Path, Token, Type, Visibility, braced, parenthesized};

impl Parse for Model {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut sub_models = vec![];

        let visibility: Visibility = input.parse()?;
        let name: Ident = input.parse()?;

        let body;
        braced!(body in input);

        let mut imported_resources = vec![];
        let mut new_resources = vec![];
        let mut daemons = vec![];

        while !body.is_empty() {
            if body.peek(Token![..]) {
                // Submodel import
                let _: Token![..] = body.parse()?;
                sub_models.push(body.parse()?);
            } else if body.peek(syn::Ident) {
                let path: Path = body.parse()?;
                if path.is_ident("react") {
                    daemons.push(parse_daemon(&body)?);
                } else {
                    // Resource declaration
                    parse_resource_declaration(
                        path,
                        &body,
                        Visibility::Inherited,
                        &mut imported_resources,
                        &mut new_resources,
                    )?;
                }
            } else {
                let visibility = body.parse()?;
                let path: Path = body.parse()?;
                parse_resource_declaration(
                    path,
                    &body,
                    visibility,
                    &mut imported_resources,
                    &mut new_resources,
                )?;
            }

            if body.peek(Token![,]) {
                let _: Token![,] = body.parse()?;
            } else if body.peek(syn::Ident) || body.peek(Token![..]) {
                return Err(body.error("Expected a comma (,) before next resource or submodel."));
            }
        }

        Ok(Model {
            visibility,
            name,
            imported_resources,
            new_resources,
            sub_models,
            daemons,
        })
    }
}

fn parse_resource_declaration(
    path: Path,
    input: ParseStream,
    visibility: Visibility,
    imported_resources: &mut Vec<Path>,
    new_resources: &mut Vec<(Visibility, Ident, Type, Option<syn::Expr>)>,
) -> syn::Result<()> {
    if input.peek(Token![:]) {
        let ident = path
            .get_ident()
            .ok_or_else(|| input.error("New resource declarations must be only a single ident."))?;
        let _: Token![:] = input.parse()?;
        let ty = input.parse()?;

        // Check for default value
        let default_expr = if input.peek(Token![=]) {
            let _: Token![=] = input.parse()?;
            Some(input.parse()?)
        } else {
            None
        };

        new_resources.push((visibility, ident.clone(), ty, default_expr));
    } else {
        if visibility != Visibility::Inherited {
            return Err(input.error("Cannot specify visibility on an imported resource."));
        }
        imported_resources.push(path);
    }
    Ok(())
}

fn parse_daemon(input: ParseStream) -> syn::Result<Daemon> {
    let resources_paren;
    parenthesized!(resources_paren in input);

    let mut resources = vec![];
    while !resources_paren.is_empty() {
        let resource = resources_paren.parse()?;
        resources.push(resource);
        if resources_paren.peek(Token![,]) {
            let _: Token![,] = resources_paren.parse()?;
        }
    }

    let function_call = input.parse()?;

    Ok(Daemon {
        resources,
        function_call,
    })
}
