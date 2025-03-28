use crate::Time;
use crate::exec::ExecEnvironment;
use crate::macro_prelude::{GroundingContinuation, GroundingUpstream, Resource, UpstreamVec};
use crate::operation::{Node, Upstream};
use crate::timeline::{Timelines, epoch_to_duration};
use anyhow::Result;
use bumpalo_herd::Member;
use hifitime::Duration;
use rayon::Scope;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::hash::Hash;
use std::ops::AddAssign;

pub trait OpsReceiver<'o> {
    /// Add an operation at the current time.
    fn push<N: Node<'o> + 'o>(&mut self, op_ctor: impl FnOnce(Placement<'o>) -> N);

    /// Move the ops cursor later in time by a relative delay.
    ///
    /// Can be either a [Duration] or a dynamic `delay!` (not yet implemented).
    fn wait<D>(&mut self, delay: D)
    where
        Placement<'o>: AddAssign<D>;

    /// Fast-forwards to the given time if it is in the future.
    ///
    /// Does nothing if it is in the past.
    fn wait_until(&mut self, _time: Time);

    /// Sets the cursor to the given time.
    fn goto(&mut self, time: Time);
}

/// A cursor and operations aggregator for inserting ops into the plan.
///
/// Within the context of an [Activity], you can move the cursor around
/// in time (including backward), then add operations with [OpsReceiver::push]
/// or the add-assign (`+=`) operator.
///
/// ## Delegating & Mutability
///
/// Since this struct contains some internal state (the location of the cursor
/// in the plan), you might need to be cautious about delegating to helper functions
/// that move the cursor around. If you want those cursor changes to be reflected
/// back in the original scope, the helper function should accept `&mut Ops` as argument;
/// if not, the helper function should take just `Ops`. This struct is [Copy], and making
/// a copy will make a new cursor that can be moved around independently.
///
/// Alternatively, if you want to decide the mutability at the call site instead of the
/// function declaration, the function can accept `impl OpsReceiver<'o>`. Both `Ops` and
/// `&mut Ops` implement [OpsReceiver], and so the function caller can decide which to pass.
#[derive(Copy, Clone)]
pub struct Ops<'v, 'o: 'v> {
    /// The current placement time that operations will be inserted at.
    pub(crate) placement: Placement<'o>,
    /// An arena allocator to store operations in.
    pub(crate) bump: &'v Member<'o>,
    /// The aggregator for operation references. The underlying [Vec]
    /// is unwrapped by the [Plan][crate::Plan] after the activity is done.
    pub(crate) operations: &'v RefCell<Vec<&'o dyn Node<'o>>>,
}

impl<'o> OpsReceiver<'o> for Ops<'_, 'o> {
    #[inline]
    fn push<N: Node<'o> + 'o>(&mut self, op_ctor: impl FnOnce(Placement<'o>) -> N) {
        let op = self.bump.alloc(op_ctor(self.placement));
        self.operations.borrow_mut().push(op);
    }

    fn wait<D>(&mut self, delay: D)
    where
        Placement<'o>: AddAssign<D>,
    {
        self.placement += delay;
    }

    fn wait_until(&mut self, _time: Time) {
        todo!()
    }

    fn goto(&mut self, time: Time) {
        self.placement = Placement::Static(epoch_to_duration(time));
    }
}

impl<'o> OpsReceiver<'o> for &mut Ops<'_, 'o> {
    fn push<N: Node<'o> + 'o>(&mut self, op_ctor: impl FnOnce(Placement<'o>) -> N) {
        (*self).push(op_ctor);
    }

    fn wait<D>(&mut self, delay: D)
    where
        Placement<'o>: AddAssign<D>,
    {
        (*self).wait(delay);
    }

    fn wait_until(&mut self, time: Time) {
        (*self).wait_until(time);
    }

    fn goto(&mut self, time: Time) {
        (*self).goto(time);
    }
}

impl<'o, N: Node<'o> + 'o, F: FnOnce(Placement<'o>) -> N> AddAssign<F> for Ops<'_, 'o> {
    fn add_assign(&mut self, rhs: F) {
        self.push(rhs);
    }
}

impl<'o, N: Node<'o> + 'o, F: FnOnce(Placement<'o>) -> N> AddAssign<F> for &mut Ops<'_, 'o> {
    fn add_assign(&mut self, rhs: F) {
        self.push(rhs);
    }
}

/// An activity, which produces into a statically-known set of operations.
/// Returns the activity's final duration and may produce errors.
#[cfg_attr(feature = "serde", typetag::serde(tag = "type"))]
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

/// The placement of an activity or operation.
///
/// It might be a statically known concrete time, or a time that is
/// decided dynamically at runtime. The user isn't expected to interact
/// with this enum directly.
#[doc(hidden)]
pub enum Placement<'o> {
    Static(Duration),
    Dynamic {
        min: Duration,
        max: Duration,
        node: &'o dyn GroundingUpstream<'o>,
    },
}

impl Clone for Placement<'_> {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for Placement<'_> {}

impl<'o> Placement<'o> {
    pub fn min(&self) -> Duration {
        match self {
            Placement::Static(start) => *start,
            Placement::Dynamic { min, .. } => *min,
        }
    }

    pub fn insert_me<R: Resource>(
        &self,
        me: &'o dyn Upstream<'o, R>,
        timelines: &Timelines<'o>,
    ) -> UpstreamVec<'o, R> {
        match self {
            Placement::Static(d) => timelines.insert_grounded::<R>(*d, me),
            Placement::Dynamic { .. } => todo!(),
        }
    }

    pub fn remove_me<R: Resource>(&self, timelines: &Timelines<'o>) -> bool {
        match *self {
            Placement::Static(d) => timelines.remove_grounded::<R>(d),
            Placement::Dynamic { min, max, .. } => timelines.remove_ungrounded::<R>(min, max),
        }
    }

    pub fn get_static(&self) -> Option<Duration> {
        match self {
            Placement::Static(d) => Some(*d),
            Placement::Dynamic { .. } => None,
        }
    }

    pub fn request_grounding<'s>(
        &'o self,
        continuation: GroundingContinuation<'o>,
        already_registered: bool,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) {
        match self {
            Placement::Static(_) => unreachable!(),
            Placement::Dynamic { node, .. } => {
                node.request_grounding(continuation, already_registered, scope, timelines, env)
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
    pub(crate) _time: Time,
    pub(crate) operations: Vec<&'o dyn Node<'o>>,
}
