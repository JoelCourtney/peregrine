use crate::macro_prelude::Grounder;
use crate::operation::grounding::peregrine_grounding;
use crate::operation::{Node, Upstream};
use crate::timeline::{Timelines, epoch_to_duration};
use crate::{Model, Time};
use anyhow::Result;
use bumpalo_herd::Member;
use hifitime::Duration;
use serde::{Deserialize, Serialize};
use std::hash::Hash;
use std::ops::{Add, AddAssign};

pub struct Ops<'v, 'o, M: Model<'o>> {
    pub(crate) placement: Placement<'o, M>,
    pub(crate) bump: &'v Member<'o>,
    pub(crate) operations: &'v mut Vec<&'o dyn Node<'o, M>>,
}

impl<'o, M: Model<'o>> Ops<'_, 'o, M> {
    #[inline]
    pub fn run<OC: OpConstructor<'o, M>>(&mut self, op: OC) {
        let op = self.bump.alloc(OC::new(self.placement));
        self.operations.push(op);
    }

    pub fn wait(&mut self, delay: impl Delay<'o, M>) {
        self.placement = delay.add_to(self.placement, &self.bump);
    }

    pub fn wait_until(&mut self, time: Time) {
        todo!()
    }

    pub fn goto(&mut self, time: Time) {
        self.placement = Placement::Static(epoch_to_duration(time));
    }
}

impl<'o, M: Model<'o>, OC: OpConstructor<'o, M>> AddAssign<OC> for Ops<'_, 'o, M> {
    fn add_assign(&mut self, rhs: OC) {
        self.run(rhs);
    }
}

pub trait Delay<'o, M: Model<'o>> {
    fn add_to(self, placement: Placement<'o, M>, bump: &Member<'o>) -> Placement<'o, M>;
}

pub trait OpConstructor<'o, M: Model<'o>> {
    fn new(grounder: impl Grounder<'o, M>) -> impl Node<'o, M> + 'o;
}

/// An activity, which decomposes into a statically-known set of operations. Implemented
/// with the [impl_activity][crate::impl_activity] macro.
pub trait Activity<'o, M: Model<'o>>: Send + Sync {
    fn run(&'o self, ops: Ops<'_, 'o, M>) -> Result<Duration>;
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
