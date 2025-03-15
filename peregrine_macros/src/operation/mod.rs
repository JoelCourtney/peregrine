mod input;
mod output;

use proc_macro2::{Ident, TokenStream};

#[derive(Debug)]
pub struct Op {
    pub reads: Vec<Ident>,
    pub writes: Vec<Ident>,
    pub read_writes: Vec<Ident>,
    body: TokenStream,
    uuid: String,
}
