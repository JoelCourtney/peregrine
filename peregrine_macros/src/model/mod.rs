mod input;
mod output;

use proc_macro2::{Ident, TokenStream};
use quote::ToTokens;
use syn::{Path, Visibility};

pub fn process_model(model: Model) -> TokenStream {
    model.into_token_stream()
}

