use crate::operation::Op;
use crate::operation::input::InteractionType::*;
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

    fn insert(&mut self, id: Ident, ty: InteractionType) -> syn::Result<()> {
        if let Some(existing) = self.get(&id) {
            // Check if we're trying to use both read and write tags for the same resource
            if *existing != ty
                && *existing != InteractionType::ReadWrite
                && ty != InteractionType::ReadWrite
            {
                return Err(syn::Error::new(
                    id.span(),
                    format!(
                        "Resource '{id}' is used with conflicting tags. \
                        If you need to read and write to the same resource, use 'm:{id}' instead of 'r:{id}' and 'w:{id}'. \
                        Only one instance of the resource needs to be tagged. The others can be left untagged if you want."
                    ),
                ));
            }
            let existing_mut = self.get_mut(&id).unwrap();
            *existing_mut = existing_mut.merge(ty);
        } else {
            self.0.insert(id, ty);
        }
        Ok(())
    }
}

impl InteractionType {
    fn merge(self, other: InteractionType) -> InteractionType {
        use InteractionType::ReadWrite;
        if self == other { self } else { ReadWrite }
    }
}

impl Parse for Op {
    fn parse(input_stream: ParseStream) -> syn::Result<Self> {
        let mut interactions = Interactions::new();

        let read_regex =
            Regex::new(r"[^[:alpha:][:digit:]_]r[[:space:]]*:[[:space:]]*(?<ident>[a-zA-Z0-9_]+)")
                .unwrap();
        let write_regex =
            Regex::new(r"[^[:alpha:][:digit:]_]w[[:space:]]*:[[:space:]]*(?<ident>[a-zA-Z0-9_]+)")
                .unwrap();
        let read_write_regex =
            Regex::new(r"[^[:alpha:][:digit:]_]m[[:space:]]*:[[:space:]]*(?<ident>[a-zA-Z0-9_]+)")
                .unwrap();
        let tag_only_regex = Regex::new(r"([^[:alpha:][:digit:]_])(r|w|m)[[:space:]]*:").unwrap();

        let mut input = input_stream.to_string();
        input.insert(0, ' ');

        for cap in read_regex.captures_iter(&input) {
            interactions.insert(format_ident!("{}", cap["ident"]), Read)?;
        }
        for cap in write_regex.captures_iter(&input) {
            interactions.insert(format_ident!("{}", cap["ident"]), Write)?;
        }
        for cap in read_write_regex.captures_iter(&input) {
            interactions.insert(format_ident!("{}", cap["ident"]), ReadWrite)?;
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

        let body = tag_only_regex.replace_all(&input, "$1").parse()?;

        input_stream.step(|_| Ok(((), Cursor::empty())))?;

        Ok(Op {
            reads,
            writes,
            read_writes,
            body,
            internal: false,
        })
    }
}
