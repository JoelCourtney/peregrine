mod input;
mod output;

use proc_macro2::Ident;
use syn::{Block, Path};

pub struct Op {
    pub(crate) context: Context,
    reads: Vec<Ident>,
    writes: Vec<Ident>,
    read_writes: Vec<Ident>,
    body: Block,
    uuid: String,
}

pub enum Context {
    Activity(Path),
    Arguments(Vec<Ident>),
    None,
}
