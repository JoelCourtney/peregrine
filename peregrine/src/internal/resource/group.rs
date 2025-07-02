use peregrine_macros::internal_op;
use std::hash::Hash;
use std::ops::IndexMut;

use crate::{Ops, Resource};

pub fn sync_single_to_group<
    GROUP: Resource,
    SINGLE: Resource,
    S: 'static + Copy + Send + Sync + Hash,
>(
    mut ops: Ops,
    which: S,
) where
    GROUP::Data: IndexMut<S, Output = SINGLE::Data>,
{
    ops += internal_op! {
        m:GROUP[which] = m:SINGLE.clone();
    }
}
