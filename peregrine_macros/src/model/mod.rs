mod input;
mod output;

use proc_macro2::Ident;
use syn::{Path, Visibility};

pub struct Model {
    visibility: Visibility,
    name: Ident,
    resources: Vec<Path>,
    sub_models: Vec<Path>,
}
