use crate::model::Model;
use proc_macro2::Ident;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Path, Token, Visibility, parenthesized};

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
        parenthesized!(body in input);

        let resources = Punctuated::<Path, Token![,]>::parse_terminated(&body)?.into_iter();

        Ok(Model {
            visibility,
            name,
            resources: resources.collect(),
            sub_models,
        })
    }
}
