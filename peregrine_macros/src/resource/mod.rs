mod input;
mod output;

use syn::{Ident, Type, Visibility};

pub use input::MultiResource;

#[derive(Debug)]
pub struct Resource {
    pub visibility: Visibility,
    pub name: Ident,
    pub data_type: Type,
    pub default_expr: Option<syn::Expr>,
    pub attrs: Vec<syn::Attribute>,
}
