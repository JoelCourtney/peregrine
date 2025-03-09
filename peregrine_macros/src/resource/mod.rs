mod input;
mod output;

use syn::{Ident, Type, Visibility};

pub struct Resource {
    visibility: Visibility,
    name: Ident,
    write_type: Type,
    history: HistoryType,
    dynamic: bool,
}

#[derive(Copy, Clone, PartialEq)]
pub enum HistoryType {
    Copy,
    Deref,
}
