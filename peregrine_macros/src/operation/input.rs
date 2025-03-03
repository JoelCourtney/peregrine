use crate::operation::input::InteractionType::*;
use crate::operation::{Context, Op};
use derive_more::{Deref, DerefMut};
use proc_macro2::Ident;
use quote::format_ident;
use regex::Regex;
use std::collections::HashMap;
use syn::buffer::Cursor;
use syn::parse::{Parse, ParseStream};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum InteractionType {
    Read,
    Write,
    ReadWrite,
}

#[derive(Debug, Deref, DerefMut)]
struct Interactions(HashMap<Ident, InteractionType>);

impl Interactions {
    fn new() -> Self {
        Self(HashMap::new())
    }

    fn insert(&mut self, id: Ident, ty: InteractionType) {
        if self.contains_key(&id) {
            let existing = self.get_mut(&id).unwrap();
            *existing = existing.merge(ty);
        } else {
            self.0.insert(id, ty);
        }
    }
}

impl InteractionType {
    fn merge(self, other: InteractionType) -> InteractionType {
        use InteractionType::ReadWrite;
        if self == other { self } else { ReadWrite }
    }
}

impl Parse for Op {
    fn parse(asdf: ParseStream) -> syn::Result<Self> {
        let mut interactions = Interactions::new();

        let read_regex =
            Regex::new(r"ref[[:space:]]*:[[:space:]]*(?<ident>[a-zA-Z0-9_]+)").unwrap();
        let write_regex =
            Regex::new(r"mut[[:space:]]*:[[:space:]]*(?<ident>[a-zA-Z0-9_]+)").unwrap();
        let read_write_regex =
            Regex::new(r"ref mut[[:space:]]*:[[:space:]]*(?<ident>[a-zA-Z0-9_]+)").unwrap();
        let tag_only_regex = Regex::new(r"(ref|mut|ref mut)[[:space:]]*:").unwrap();

        let input = asdf.to_string();

        for cap in read_regex.captures_iter(&input) {
            interactions.insert(format_ident!("{}", cap["ident"]), Read);
        }
        for cap in write_regex.captures_iter(&input) {
            interactions.insert(format_ident!("{}", cap["ident"]), Write);
        }
        for cap in read_write_regex.captures_iter(&input) {
            interactions.insert(format_ident!("{}", cap["ident"]), ReadWrite);
        }

        let mut reads = vec![];
        let mut writes = vec![];
        let mut read_writes = vec![];

        for (ident, ty) in interactions.0 {
            match ty {
                Read => reads.push(ident),
                Write => writes.push(ident),
                ReadWrite => read_writes.push(ident),
            }
        }

        let body = tag_only_regex.replace_all(&input, "").parse()?;

        asdf.step(|_| Ok(((), Cursor::empty())))?;

        Ok(Op {
            context: Context::None,
            reads,
            writes,
            read_writes,
            body,
            uuid: uuid::Uuid::new_v4().to_string().replace("-", "_"),
        })
    }
}
