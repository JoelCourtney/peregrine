mod input;
mod output;

use syn::{Ident, Type, Visibility};

pub struct Resource {
    visibility: Visibility,
    name: Ident,
    write_type: Type,
}
