use crate::resource::Resource;
use syn::Token;
use syn::parse::{Parse, ParseStream};

impl Parse for Resource {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let visibility = input.parse()?;

        let name = input.parse()?;

        let _: Token![:] = input.parse()?;

        let write_type = input.parse()?;

        Ok(Resource {
            visibility,
            name,
            write_type,
        })
    }
}
