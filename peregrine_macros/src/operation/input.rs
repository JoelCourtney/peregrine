use proc_macro::TokenTree;
use std::collections::HashMap;
use derive_more::Deref;
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream, StepCursor};
use syn::{parenthesized, parse_macro_input, Block, Error, Expr, Token};
use syn::parse::discouraged::Speculative;
use syn::token::Token;
use crate::operation::input::InteractionType::*;
use crate::operation::{Context, Op};

#[derive(Copy, Clone, Eq, PartialEq)]
enum InteractionType {
    Read,
    Write,
    ReadWrite
}

#[derive(Deref)]
struct Interactions(HashMap<Ident, InteractionType>);

impl Interactions {
    fn new() -> Self {
        Self(HashMap::new())
    }

    fn merge(&mut self, other: Interactions) {
        for (id, ty) in other.0 {
            if self.contains_key(&id) {
                self.get_mut(&id).unwrap().merge(ty);
            } else {
                self.insert(id, ty);
            }
        }
    }
}

impl InteractionType {
    fn merge(self, other: InteractionType) -> InteractionType {
        use InteractionType::ReadWrite;
        if self == other {
            self
        } else {
            ReadWrite
        }
    }

    fn to_idents(self) -> Vec<Ident> {
        match self {
            Read => vec![format_ident!("ref")],
            Write => vec![format_ident!("mut")],
            ReadWrite => vec![format_ident!("ref"), format_ident!("mut")],
        }
    }
}

struct RawOpParse(Interactions, TokenStream);

impl Parse for RawOpParse {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        fn filter(mut cursor: StepCursor) -> syn::Result<(RawOpParse, StepCursor)> {
            #[derive(Copy, Clone, Eq, PartialEq)]
            enum FilterState {
                Dormant,
                InProgress(InteractionType),
                Ready(InteractionType)
            }
            use FilterState::*;

            let mut interactions = Interactions::new();
            let mut filtered = TokenStream::new();

            let mut state = Dormant;

            let mut rest = *cursor;
            while let Some((tt, next)) = rest.token_tree() {
                match (state, tt.clone()) {
                    (Dormant, TokenTree::Ident(i)) => {
                        match i.to_string().as_str() {
                            "ref" => { state = InProgress(Read); }
                            "mut" => { state = InProgress(Write); }
                            _ => { filtered.extend([tt]); }
                        }
                    }
                    (InProgress(Read), TokenTree::Ident(i)) => {
                        match i.to_string().as_str() {
                            "mut" => { state = InProgress(ReadWrite); }
                            _ => {
                                state = Dormant;
                                filtered.extend([tt]);
                            }
                        }
                    }
                    (InProgress(int), TokenTree::Punct(p)) => {
                        if p.as_char() == ':' {
                            state = Ready(int);
                        } else {
                            filtered.extend(
                                int.to_idents()
                            );
                            state = Dormant;
                        }
                    }
                    (Ready(int), TokenTree::Ident(i)) => {
                        filtered.extend([i.clone()]);
                        interactions.insert(i.into(), int);
                    }
                    (state, TokenTree::Group(p)) => {
                        if let InProgress(int) | Ready(int) = state {
                            filtered.extend(int.to_idents());
                        }
                        let RawOpParse(sub_interactions, sub_filter) = syn::parse(p.stream())?;
                        interactions.merge(sub_interactions);
                        filtered.extend(sub_filter);
                    }
                    (state, tt) => {
                        if let InProgress(int) | Ready(int) = state {
                            filtered.extend(int.to_idents());
                        }
                        filtered.extend([tt]);
                    }
                }
                rest = next;
            }
            Ok((RawOpParse(interactions, filtered), StepCursor::em))
        }

        input.step(|cursor| filter(cursor))
    }
}

impl Parse for Op {
    fn parse(input: ParseStream) -> syn::Result<Self> {



        let mut reads = vec![];
        let mut writes = vec![];
        let mut read_writes = vec![];

        for (ident, ty) in interactions {
            match  ty {
                Read => reads.push(ident),
                Write => writes.push(ident),
                ReadWrite => read_writes.push(ident),
            }
        }

        let body: Block = input.parse()?;

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
