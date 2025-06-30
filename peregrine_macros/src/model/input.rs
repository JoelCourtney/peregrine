use crate::model::{Daemon, Model};
use proc_macro2::Ident;
use syn::parse::{Parse, ParseStream};
use syn::{Token, Visibility, braced, parenthesized};

impl Model {
    fn parse_extras(input: ParseStream) -> syn::Result<Self> {
        let mut sub_models = vec![];
        let mut daemons = vec![];
        let mut imported_resources = vec![];

        // Now parse submodels and daemons outside the model block
        while !input.is_empty() {
            if input.peek(Token![mod])
                || input.peek(Token![use])
                || (input.peek(syn::Ident)
                    && input.fork().parse::<Ident>().is_ok_and(|id| id == "react"))
            {
                // Continue parsing
            } else {
                // Stop if we hit something else (like pub PotatoSat)
                break;
            }
            if input.peek(Token![mod]) {
                let _: Token![mod] = input.parse()?;
                sub_models.push(input.parse()?);
            } else if input.peek(Token![use]) {
                let _: Token![use] = input.parse()?;
                imported_resources.push(input.parse()?);
            } else if input.peek(syn::Ident) && input.fork().parse::<Ident>()? == "react" {
                let daemon = parse_daemon(input)?;
                daemons.push(daemon);
            } else {
                return Err(input.error(
                    "Expected `use` for submodel import or `react` for daemon declaration.",
                ));
            }

            let _: Token![;] = input.parse()?;
        }
        Ok(Model {
            visibility: Visibility::Inherited,
            name: Ident::new("placeholder", proc_macro2::Span::call_site()),
            imported_resources,
            new_resources: vec![],
            sub_models,
            daemons,
        })
    }
}

impl Parse for Model {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut result = Self::parse_extras(input)?;

        // Parse the model block
        result.visibility = input.parse()?;
        result.name = input.parse()?;

        let body;
        braced!(body in input);

        while !body.is_empty() {
            result.new_resources.push(body.parse()?);
        }

        let post_extras = Self::parse_extras(input)?;
        result.sub_models.extend(post_extras.sub_models);
        result.daemons.extend(post_extras.daemons);
        result
            .imported_resources
            .extend(post_extras.imported_resources);

        Ok(result)
    }
}

fn parse_daemon(input: ParseStream) -> syn::Result<Daemon> {
    let lookahead = input.fork();
    let ident: Ident = lookahead.parse()?;
    if ident != "react" {
        return Err(input.error("Expected 'react' for daemon declaration."));
    }
    let _: Ident = input.parse()?; // consume 'react'

    let resources_paren;
    parenthesized!(resources_paren in input);

    let mut resources = vec![];
    let mut react_to_all = false;

    // Check if the first token is a star (*)
    if resources_paren.peek(Token![*]) {
        let _: Token![*] = resources_paren.parse()?;
        react_to_all = true;

        // Ensure there's nothing else in the parentheses after the star
        if !resources_paren.is_empty() {
            return Err(resources_paren.error("Expected only '*' in react(*) syntax"));
        }
    } else {
        // Parse the list of resources as before
        while !resources_paren.is_empty() {
            let resource = resources_paren.parse()?;
            resources.push(resource);
            if resources_paren.peek(Token![,]) {
                let _: Token![,] = resources_paren.parse()?;
            }
        }
    }

    let function_call = input.parse()?;

    Ok(Daemon {
        resources,
        function_call,
        react_to_all,
    })
}
