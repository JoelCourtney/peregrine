#![doc(hidden)]

pub mod initial_conditions;
pub mod ungrounded;

use crate::Model;
use crate::exec::ExecEnvironment;
use crate::operation::ungrounded::{Marked, MarkedValue, peregrine_grounding};
use crate::resource::Resource;
use crate::timeline::Timelines;
use anyhow::Result;
use derive_more::with_trait::Error as DeriveError;
use hifitime::Duration;
use rayon::Scope;
use smallvec::SmallVec;
use std::fmt::{Debug, Display, Formatter};

pub type InternalResult<T> = Result<T, ObservedErrorOutput>;

pub trait Node<'o, M: Model<'o> + 'o>: Sync {
    fn insert_self(&'o self, timelines: &mut Timelines<'o, M>) -> Result<()>;
    fn remove_self(&self, timelines: &mut Timelines<'o, M>) -> Result<()>;
}

pub trait Downstream<'o, R: Resource<'o>, M: Model<'o> + 'o>: Sync {
    fn respond<'s>(
        &'o self,
        value: InternalResult<(u64, R::Read)>,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o, M>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's;

    fn clear_cache(&self);
    fn clear_upstream(&self, time_of_change: Option<Duration>) -> bool;
}

pub trait Upstream<'o, R: Resource<'o>, M: Model<'o> + 'o>: Sync {
    fn request<'s>(
        &'o self,
        continuation: Continuation<'o, R, M>,
        already_registered: bool,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o, M>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's;

    fn notify_downstreams(&self, time_of_change: Duration);
    fn register_downstream_early(&self, downstream: &'o dyn Downstream<'o, R, M>);
}

pub enum Continuation<'o, R: Resource<'o>, M: Model<'o> + 'o> {
    Node(&'o dyn Downstream<'o, R, M>),
    MarkedNode(usize, &'o dyn Downstream<'o, Marked<'o, R>, M>),
    Root(oneshot::Sender<InternalResult<R::Read>>),
}

impl<'o, R: Resource<'o>, M: Model<'o> + 'o> Continuation<'o, R, M> {
    pub fn run<'s>(
        self,
        value: InternalResult<(u64, R::Read)>,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o, M>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's,
    {
        match self {
            Continuation::Node(n) => n.respond(value, scope, timelines, env),
            Continuation::MarkedNode(marker, n) => n.respond(
                value.map(|(hash, when)| {
                    (
                        hash,
                        MarkedValue {
                            marker,
                            value: when,
                        },
                    )
                }),
                scope,
                timelines,
                env,
            ),
            Continuation::Root(s) => s.send(value.map(|r| r.1)).unwrap(),
        }
    }

    pub fn copy_node(&self) -> Option<Self> {
        match &self {
            Continuation::Node(n) => Some(Continuation::Node(*n)),
            Continuation::MarkedNode(m, n) => Some(Continuation::MarkedNode(*m, *n)),
            _ => None,
        }
    }

    pub fn to_downstream(&self) -> Option<MaybeMarkedDownstream<'o, R, M>> {
        match self {
            Continuation::Node(n) => Some((*n).into()),
            Continuation::MarkedNode(_, n) => Some((*n).into()),
            _ => None,
        }
    }
}

pub struct OperationState<O, C, D> {
    pub response_counter: u8,
    pub status: OperationStatus<O>,
    pub continuations: SmallVec<C, 1>,
    pub downstreams: SmallVec<D, 1>,
}

impl<O, C, D> OperationState<O, C, D> {
    fn new() -> Self {
        Self {
            response_counter: 0,
            status: OperationStatus::Dormant,
            continuations: SmallVec::new(),
            downstreams: SmallVec::new(),
        }
    }
}

impl<O, C, D> Default for OperationState<O, C, D> {
    fn default() -> Self {
        Self::new()
    }
}

pub enum OperationStatus<O> {
    Dormant,
    Working,
    Done(InternalResult<O>),
}

impl<O: Copy> OperationStatus<O> {
    pub fn unwrap_done(&self) -> InternalResult<O> {
        match self {
            OperationStatus::Done(r) => *r,
            _ => panic!("tried to unwrap an operation result that wasn't done"),
        }
    }
}

pub enum MaybeMarkedDownstream<'o, R: Resource<'o>, M: Model<'o>> {
    Unmarked(&'o dyn Downstream<'o, R, M>),
    Marked(&'o dyn Downstream<'o, Marked<'o, R>, M>),
}

impl<'o, R: Resource<'o>, M: Model<'o>> MaybeMarkedDownstream<'o, R, M> {
    pub fn clear_upstream(&self, time_of_change: Option<Duration>) -> bool {
        match self {
            MaybeMarkedDownstream::Unmarked(n) => n.clear_upstream(time_of_change),
            MaybeMarkedDownstream::Marked(n) => n.clear_upstream(time_of_change),
        }
    }

    pub fn clear_cache(&self) {
        match self {
            MaybeMarkedDownstream::Unmarked(n) => n.clear_cache(),
            MaybeMarkedDownstream::Marked(n) => n.clear_cache(),
        }
    }
}

impl<'o, R: Resource<'o>, M: Model<'o>> From<&'o dyn Downstream<'o, R, M>>
    for MaybeMarkedDownstream<'o, R, M>
{
    fn from(value: &'o dyn Downstream<'o, R, M>) -> Self {
        MaybeMarkedDownstream::Unmarked(value)
    }
}

impl<'o, R: Resource<'o>, M: Model<'o>> From<&'o dyn Downstream<'o, Marked<'o, R>, M>>
    for MaybeMarkedDownstream<'o, R, M>
{
    fn from(value: &'o dyn Downstream<'o, Marked<'o, R>, M>) -> Self {
        MaybeMarkedDownstream::Marked(value)
    }
}

/// An internal marker error to signify that a node attempted to read
/// from an upstream node that had already computed an error.
///
/// This is to avoid duplicating the same error many times across all
/// branches of the graph. Instead, the true error is only returned once,
/// by the original task that computed it,
/// and all subsequent reads return this struct, which is filtered out
/// by `plan.view`.
#[derive(Copy, Clone, Debug, Default, DeriveError)]
pub struct ObservedErrorOutput;

impl Display for ObservedErrorOutput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "encountered a stale error from a previous run")
    }
}

pub type UpstreamVec<'o, R, M> = SmallVec<&'o dyn Upstream<'o, R, M>, 2>;

pub trait Grounder<'o, M: Model<'o> + 'o>: Upstream<'o, peregrine_grounding, M> {
    fn insert_me<R: Resource<'o>>(
        &self,
        me: &'o dyn Upstream<'o, R, M>,
        timelines: &mut Timelines<'o, M>,
    ) -> UpstreamVec<'o, R, M>;
    fn remove_me<R: Resource<'o>>(&self, timelines: &mut Timelines<'o, M>) -> bool;

    fn min(&self) -> Duration;
    fn get_static(&self) -> Option<Duration>;
}

impl<'o, M: Model<'o> + 'o> Upstream<'o, peregrine_grounding, M> for Duration {
    fn request<'s>(
        &'o self,
        continuation: Continuation<'o, peregrine_grounding, M>,
        _already_registered: bool,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o, M>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's,
    {
        continuation.run(Ok((0, *self)), scope, timelines, env);
    }

    fn notify_downstreams(&self, _time_of_change: Duration) {
        unreachable!()
    }

    fn register_downstream_early(
        &self,
        _downstream: &'o dyn Downstream<'o, peregrine_grounding, M>,
    ) {
        unreachable!()
    }
}

impl<'o, M: Model<'o> + 'o> Grounder<'o, M> for Duration {
    fn insert_me<R: Resource<'o>>(
        &self,
        me: &'o dyn Upstream<'o, R, M>,
        timelines: &mut Timelines<'o, M>,
    ) -> UpstreamVec<'o, R, M> {
        timelines.insert_grounded::<R>(*self, me)
    }

    fn remove_me<R: Resource<'o>>(&self, timelines: &mut Timelines<'o, M>) -> bool {
        timelines.remove_grounded::<R>(*self)
    }

    fn min(&self) -> Duration {
        *self
    }

    fn get_static(&self) -> Option<Duration> {
        Some(*self)
    }
}
