use crate::resource::{GroupResource, Resource, SingleResource};
use std::collections::HashMap;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Ident, Token, Visibility, braced};

pub struct MultiResource {
    pub resources: Vec<Resource>,
}

impl Parse for Resource {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let visibility: Visibility = input.parse()?;

        // Parse the identifier pattern, which might contain asterisks
        let mut name_parts = Vec::new();
        let mut has_asterisk = false;

        // Keep parsing until we hit a colon
        while !input.peek(Token![:]) {
            if input.peek(Token![*]) {
                let _: Token![*] = input.parse()?;
                name_parts.push("*".to_string());
                has_asterisk = true;
            } else {
                let ident: Ident = input.parse()?;
                name_parts.push(ident.to_string());
            }
        }

        // Reconstruct the name pattern
        let name_pattern = name_parts.join("");

        let _: Token![:] = input.parse()?;
        let data_type = input.parse()?;

        if has_asterisk {
            // Resource group syntax
            let default_expr = if input.peek(Token![=]) {
                let _: Token![=] = input.parse()?;
                Some(input.parse()?)
            } else {
                None
            };

            let _: Token![;] = input.parse()?;

            // Parse the group members list
            let content;
            braced!(content in input);

            let mut members = Vec::new();
            let mut individual_defaults = HashMap::new();

            if default_expr.is_some() {
                // Simple member list: {a, b, c}
                while !content.is_empty() {
                    let member: Ident = content.parse()?;
                    members.push(member);

                    if content.peek(Token![,]) {
                        let _: Token![,] = content.parse()?;
                    }
                }
            } else {
                // Individual defaults: {a: false, b: true}
                while !content.is_empty() {
                    let member: Ident = content.parse()?;
                    let _: Token![:] = content.parse()?;
                    let default: syn::Expr = content.parse()?;

                    individual_defaults.insert(member.to_string(), default);
                    members.push(member);

                    if content.peek(Token![,]) {
                        let _: Token![,] = content.parse()?;
                    }
                }
            }

            Ok(Resource::Group(GroupResource {
                visibility,
                name_pattern,
                data_type,
                default_expr,
                attrs,
                members,
                individual_defaults,
            }))
        } else {
            // Regular single resource syntax
            let default_expr = if input.peek(Token![=]) {
                let _: Token![=] = input.parse()?;
                Some(input.parse()?)
            } else {
                None
            };

            let _: Token![;] = input.parse()?;

            let name = Ident::new(&name_pattern, proc_macro2::Span::call_site());

            Ok(Resource::Single(SingleResource {
                visibility,
                name,
                data_type,
                default_expr,
                attrs,
            }))
        }
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
