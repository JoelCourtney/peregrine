use crate::Model;
use crate::operation::grounding::peregrine_grounding;
use crate::operation::{Node, Upstream};
use anyhow::Result;
use bumpalo_herd::Member;
use hifitime::Duration;
use serde::{Deserialize, Serialize};
use std::hash::Hash;
use std::ops::Add;

/// An activity, which decomposes into a statically-known set of operations. Implemented
/// with the [impl_activity][crate::impl_activity] macro.
pub trait Activity<'o, M: Model<'o>>: Send + Sync {
    fn decompose(
        &'o mut self,
        start: Placement<'o, M>,
        bump: Member<'o>,
    ) -> Result<(Duration, Vec<&'o dyn Node<'o, M>>)>;
}

pub trait StaticActivity: Hash {
    const LABEL: &'static str;
}

/// A unique activity ID.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, Debug)]
pub struct ActivityId(u32);
impl ActivityId {
    pub fn new(id: u32) -> ActivityId {
        ActivityId(id)
    }
}

pub enum Placement<'o, M: Model<'o>> {
    Static(Duration),
    Dynamic {
        min: Duration,
        max: Duration,
        node: &'o dyn Upstream<'o, peregrine_grounding, M>,
    },
}

impl<'o, M: Model<'o>> Clone for Placement<'o, M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'o, M: Model<'o>> Copy for Placement<'o, M> {}

impl<'o, M: Model<'o>> Placement<'o, M> {
    pub fn unwrap_node(&self) -> &dyn Upstream<'o, peregrine_grounding, M> {
        match self {
            Placement::Static(_) => panic!("tried to unwrap a static grounding"),
            Placement::Dynamic { node, .. } => *node,
        }
    }

    pub fn min(&self) -> Duration {
        match self {
            Placement::Static(start) => *start,
            Placement::Dynamic { min, .. } => *min,
        }
    }
}

impl<'o, M: Model<'o>> Add<Duration> for Placement<'o, M> {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self::Output {
        match self {
            Placement::Static(start) => Placement::Static(start + rhs),
            Placement::Dynamic { min, max, node } => Placement::Dynamic {
                min: min + rhs,
                max: max + rhs,
                node,
            },
        }
    }
}

pub(crate) struct DecomposedActivity<'o, M> {
    pub(crate) activity: *mut dyn Activity<'o, M>,
    pub(crate) operations: Vec<&'o dyn Node<'o, M>>,
}
