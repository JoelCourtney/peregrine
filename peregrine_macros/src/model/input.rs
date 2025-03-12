use crate::model::Model;
use proc_macro2::Ident;
use syn::parse::{Parse, ParseStream};
use syn::{Path, Token, Visibility, braced};

impl Parse for Model {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut sub_models = vec![];

        while input.peek(Token![use]) {
            let _: Token![use] = input.parse()?;
            sub_models.push(input.parse()?);
            let _: Token![;] = input.parse()?;
        }

        let visibility: Visibility = input.parse()?;
        let name: Ident = input.parse()?;

        let body;
        braced!(body in input);

        let mut imported_resources = vec![];
        let mut new_resources = vec![];

        while !body.is_empty() {
            let visibility = body.parse()?;
            let path: Path = body.parse()?;
            if body.peek(Token![:]) {
                let ident = path.get_ident().ok_or_else(|| {
                    body.error("New resource declarations must be only a single ident.")
                })?;
                let _: Token![:] = body.parse()?;
                let ty = body.parse()?;
                new_resources.push((visibility, ident.clone(), ty));
            } else {
                if visibility != Visibility::Inherited {
                    return Err(body.error("Cannot specify visibility on an imported resource."));
                }
                imported_resources.push(path);
            }
            if body.peek(Token![,]) {
                let _: Token![,] = body.parse()?;
            } else if body.peek(syn::Ident) {
                return Err(body.error("Expected a comma (,) before next resource."));
            }
        }

        Ok(Model {
            visibility,
            name,
            imported_resources,
            new_resources,
            sub_models,
        })
    }
}
