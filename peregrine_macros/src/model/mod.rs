mod input;
mod output;

use proc_macro2::Ident;
use syn::{Path, Type, Visibility};

pub struct Model {
    visibility: Visibility,
    name: Ident,
    imported_resources: Vec<Path>,
    new_resources: Vec<(Visibility, Ident, Type, Option<syn::Expr>)>,
    sub_models: Vec<Path>,
    daemons: Vec<Daemon>,
}

#[derive(Debug, Clone)]
pub struct Daemon {
    pub resources: Vec<Path>,
    pub function_call: syn::ExprCall,
}
