use crate::Time;
use crate::exec::ExecEnvironment;
use crate::macro_prelude::{Continuation, Resource, UpstreamVec};
use crate::operation::grounding::peregrine_grounding;
use crate::operation::{Node, Upstream};
use crate::timeline::{Timelines, epoch_to_duration};
use anyhow::Result;
use bumpalo_herd::Member;
use hifitime::Duration;
use rayon::Scope;
use serde::{Deserialize, Serialize};
use std::hash::Hash;
use std::ops::AddAssign;

pub struct Ops<'v, 'o: 'v> {
    pub(crate) placement: Placement<'o>,
    pub(crate) bump: &'v Member<'o>,
    pub(crate) operations: &'v mut Vec<&'o dyn Node<'o>>,
}

impl<'o> Ops<'_, 'o> {
    #[inline]
    pub fn run<N: Node<'o> + 'o>(&mut self, op_ctor: impl FnOnce(Placement<'o>) -> N) {
        let op = self.bump.alloc(op_ctor(self.placement));
        self.operations.push(op);
    }

    pub fn wait<D>(&mut self, delay: D)
    where
        Placement<'o>: AddAssign<D>,
    {
        self.placement += delay;
    }

    pub fn wait_until(&mut self, _time: Time) {
        todo!()
    }

    pub fn goto(&mut self, time: Time) {
        self.placement = Placement::Static(epoch_to_duration(time));
    }
}

impl<'o, N: Node<'o> + 'o, F: FnOnce(Placement<'o>) -> N> AddAssign<F> for Ops<'_, 'o> {
    fn add_assign(&mut self, rhs: F) {
        self.run(rhs);
    }
}

/// An activity, which decomposes into a statically-known set of operations. Implemented
/// with the [impl_activity][crate::impl_activity] macro.
pub trait Activity: Send + Sync {
    fn run<'o>(&'o self, ops: Ops<'_, 'o>) -> Result<Duration>;
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

pub enum Placement<'o> {
    Static(Duration),
    Dynamic {
        min: Duration,
        max: Duration,
        node: &'o dyn Upstream<'o, peregrine_grounding>,
    },
}

impl Clone for Placement<'_> {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for Placement<'_> {}

impl<'o> Placement<'o> {
    pub fn unwrap_node(&self) -> &dyn Upstream<'o, peregrine_grounding> {
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

    pub fn insert_me<R: Resource>(
        &self,
        me: &'o dyn Upstream<'o, R>,
        timelines: &mut Timelines<'o>,
    ) -> UpstreamVec<'o, R> {
        match self {
            Placement::Static(d) => timelines.insert_grounded::<R>(*d, me),
            Placement::Dynamic { .. } => todo!(),
        }
    }

    pub fn remove_me<R: Resource>(&self, timelines: &mut Timelines<'o>) -> bool {
        match self {
            Placement::Static(d) => timelines.remove_grounded::<R>(*d),
            Placement::Dynamic { .. } => todo!(),
        }
    }

    pub fn get_static(&self) -> Option<Duration> {
        match self {
            Placement::Static(d) => Some(*d),
            Placement::Dynamic { .. } => None,
        }
    }

    pub fn request<'s>(
        &'o self,
        continuation: Continuation<'o, peregrine_grounding>,
        already_registered: bool,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) {
        match self {
            Placement::Static(_) => unreachable!(),
            Placement::Dynamic { node, .. } => {
                node.request(continuation, already_registered, scope, timelines, env)
            }
        }
    }
}

impl AddAssign<Duration> for Placement<'_> {
    fn add_assign(&mut self, rhs: Duration) {
        match self {
            Placement::Static(start) => *start += rhs,
            Placement::Dynamic { min, max, .. } => {
                *min += rhs;
                *max += rhs;
            }
        }
    }
}

pub(crate) struct DecomposedActivity<'o> {
    pub(crate) activity: *mut dyn Activity,
    pub(crate) operations: Vec<&'o dyn Node<'o>>,
}
