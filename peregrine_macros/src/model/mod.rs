mod input;
mod output;

use crate::resource::Resource;
use proc_macro2::Ident;
use syn::{Path, Visibility};

pub struct Model {
    visibility: Visibility,
    name: Ident,
    imported_resources: Vec<Path>,
    new_resources: Vec<Resource>,
    sub_models: Vec<Path>,
    daemons: Vec<Daemon>,
}

#[derive(Debug, Clone)]
pub struct Daemon {
    pub resources: Vec<Path>,
    pub function_call: syn::ExprCall,
    pub react_to_all: bool,
}
