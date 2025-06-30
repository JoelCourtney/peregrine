mod input;
mod output;

use std::collections::HashMap;
use syn::{Ident, Type, Visibility};

pub use input::MultiResource;

#[derive(Debug)]
pub enum Resource {
    Single(SingleResource),
    Group(GroupResource),
}

#[derive(Debug)]
pub struct SingleResource {
    pub visibility: Visibility,
    pub name: Ident,
    pub data_type: Type,
    pub default_expr: Option<syn::Expr>,
    pub attrs: Vec<syn::Attribute>,
}

#[derive(Debug)]
pub struct GroupResource {
    pub visibility: Visibility,
    pub name_pattern: String, // Pattern with asterisk
    pub data_type: Type,
    pub default_expr: Option<syn::Expr>, // Shared default for all members
    pub attrs: Vec<syn::Attribute>,
    pub members: Vec<Ident>,
    pub individual_defaults: HashMap<String, syn::Expr>, // Individual defaults
}
