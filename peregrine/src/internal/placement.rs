use crate::internal::exec::ExecEnvironment;
use crate::internal::operation;
use crate::internal::operation::grounding::peregrine_grounding;
use crate::internal::operation::{Continuation, Node, Upstream};
use crate::internal::timeline::Timelines;
use crate::public::activity::Activity;
use crate::{Data, MaybeHash, Time};
use bumpalo_herd::Member;
use hifitime::Duration;
use operation::grounding::{Delay, GroundingContinuation};
use rayon::Scope;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::ops::{Add, AddAssign};

pub trait StaticActivity: Hash {
    const LABEL: &'static str;
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DenseTime {
    pub when: Duration,
    pub order: u64,
}

impl MaybeHash for DenseTime {
    fn is_hashable(&self) -> bool {
        true
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.hash(state);
    }
}

impl<'h> Data<'h> for DenseTime {
    type Read = Self;
    type Sample = Self;

    fn to_read(&self, _written: Time) -> Self::Read {
        *self
    }

    fn from_read(read: Self::Read, _now: Time) -> Self {
        read
    }

    fn sample(read: Self::Read, _now: Time) -> Self::Sample {
        read
    }
}

impl DenseTime {
    pub fn first_at(when: Duration) -> Self {
        DenseTime { when, order: 0 }
    }
    pub fn last_at(when: Duration) -> Self {
        DenseTime {
            when,
            order: u64::MAX,
        }
    }
}

impl Add<Duration> for DenseTime {
    type Output = Self;

    fn add(mut self, rhs: Duration) -> Self::Output {
        self.when += rhs;
        self
    }
}

impl AddAssign<Duration> for DenseTime {
    fn add_assign(&mut self, rhs: Duration) {
        self.when += rhs;
    }
}

/// The placement of an activity or operation.
///
/// It might be a statically known concrete time, or a time that is
/// decided dynamically at runtime. The user isn't expected to interact
/// with this enum directly.
#[doc(hidden)]
pub enum Placement<'o> {
    Static(DenseTime),
    Dynamic {
        min: DenseTime,
        max: DenseTime,
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
    pub fn min(&self) -> DenseTime {
        match self {
            Placement::Static(start) => *start,
            Placement::Dynamic { min, .. } => *min,
        }
    }

    pub fn max(&self) -> DenseTime {
        match self {
            Placement::Static(start) => *start,
            Placement::Dynamic { max, .. } => *max,
        }
    }

    pub fn get_static(&self) -> Option<DenseTime> {
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

    pub fn set_order(&mut self, order: u64) {
        match self {
            Placement::Static(p) => p.order = order,
            Placement::Dynamic { min, max, .. } => {
                min.order = order;
                max.order = order;
            }
        }
    }

    pub fn get_order(&self) -> u64 {
        match self {
            Placement::Static(p) => p.order,
            Placement::Dynamic { min, max, .. } => {
                assert_eq!(min.order, max.order);
                min.order
            }
        }
    }
}

impl<'o> Add<Duration> for Placement<'o> {
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
    pub(crate) operations: Vec<&'o dyn Node<'o>>,
}
