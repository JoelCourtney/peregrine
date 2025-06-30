use crate::resource::Resource;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Token, Visibility};

pub struct MultiResource {
    pub resources: Vec<Resource>,
}

impl Parse for Resource {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let visibility: Visibility = input.parse()?;
        let name = input.parse()?;
        let _: Token![:] = input.parse()?;
        let data_type = input.parse()?;

        let default_expr = if input.peek(Token![=]) {
            let _: Token![=] = input.parse()?;
            Some(input.parse()?)
        } else {
            None
        };

        let _: Token![;] = input.parse()?;
        Ok(Resource {
            visibility,
            name,
            data_type,
            default_expr,
            attrs,
        })
    }
}

impl Parse for MultiResource {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut resources = Vec::new();

        while !input.is_empty() {
            let resource: Resource = input.parse()?;
            resources.push(resource);
        }

        Ok(MultiResource { resources })
    }
}
