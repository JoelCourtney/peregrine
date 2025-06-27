use crate::Time;
use crate::internal::exec::ExecEnvironment;
use crate::internal::operation;
use crate::internal::operation::grounding::peregrine_grounding;
use crate::internal::operation::{Continuation, Node, Upstream, UpstreamVec};
use crate::internal::timeline::Timelines;
use crate::public::activity::Activity;
use crate::public::resource::Resource;
use bumpalo_herd::Member;
use hifitime::Duration;
use operation::grounding::{Delay, GroundingContinuation};
use rayon::Scope;
use std::hash::Hash;
use std::ops::AddAssign;

pub trait StaticActivity: Hash {
    const LABEL: &'static str;
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
    pub fn min(&self) -> Duration {
        match self {
            Placement::Static(start) => *start,
            Placement::Dynamic { min, .. } => *min,
        }
    }

    pub fn max(&self) -> Duration {
        match self {
            Placement::Static(start) => *start,
            Placement::Dynamic { max, .. } => *max,
        }
    }

    pub fn insert_me<R: Resource>(
        &self,
        me: &'o dyn Upstream<'o, R>,
        timelines: &Timelines<'o>,
    ) -> UpstreamVec<'o, R> {
        match self {
            Placement::Static(d) => timelines.insert_grounded::<R>(*d, me),
            Placement::Dynamic { min, max, .. } => timelines.insert_ungrounded::<R>(*min, *max, me),
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
            Placement::Dynamic { node, .. } => node.request(
                Continuation::GroundingWrapper(continuation),
                already_registered,
                scope,
                timelines,
                env,
            ),
        }
    }
}

impl<'v, 'o: 'v> AddAssign<(Duration, &'v Member<'o>)> for Placement<'o> {
    fn add_assign(&mut self, (rhs, _): (Duration, &'v Member<'o>)) {
        match self {
            Placement::Static(start) => *start += rhs,
            Placement::Dynamic { min, max, .. } => {
                *min += rhs;
                *max += rhs;
            }
        }
    }
}

impl<'v, 'o: 'v, F: FnOnce(Placement<'o>) -> Delay<U>, U: Upstream<'o, peregrine_grounding> + 'o>
    AddAssign<(F, &'v Member<'o>)> for Placement<'o>
{
    fn add_assign(&mut self, (rhs, bump): (F, &'v Member<'o>)) {
        let delay = rhs(*self);
        match self {
            Placement::Static(start) => {
                *self = Placement::Dynamic {
                    min: *start + delay.min,
                    max: *start + delay.max,
                    node: bump.alloc(delay.node),
                }
            }
            Placement::Dynamic { min, max, .. } => {
                *self = Placement::Dynamic {
                    min: *min + delay.min,
                    max: *max + delay.max,
                    node: bump.alloc(delay.node),
                }
            }
        }
    }
}

pub(crate) struct DecomposedActivity<'o> {
    pub(crate) activity: *mut dyn Activity,
    pub(crate) _time: Time,
    pub(crate) operations: Vec<&'o dyn Node<'o>>,
}
