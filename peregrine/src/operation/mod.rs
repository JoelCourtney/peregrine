#![doc(hidden)]

pub mod grounding;
pub mod initial_conditions;

use crate as peregrine;
use crate::Model;
use crate::exec::ExecEnvironment;
use crate::resource::Resource;
use crate::timeline::Timelines;
use anyhow::Result;
use derive_more::with_trait::Error as DeriveError;
use hifitime::Duration;
use rayon::Scope;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;

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

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "peregrine::reexports::serde")]
pub enum Marked<'o, R: Resource<'o>> {
    Unit,
    Phantom(PhantomData<&'o R>),
}

impl<'o, R: 'o + Resource<'o>> Resource<'o> for Marked<'o, R> {
    const LABEL: &'static str = R::LABEL;
    const STATIC: bool = R::STATIC;
    const ID: u64 = peregrine_macros::random_u64!();
    type Read = MarkedValue<R::Read>;
    type Write = MarkedValue<R::Write>;
    type History = ();
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MarkedValue<T> {
    pub(crate) marker: usize,
    pub(crate) value: T,
}

impl<T: Copy + Clone> Copy for MarkedValue<T> {}
impl<T: Clone> Clone for MarkedValue<T> {
    fn clone(&self) -> Self {
        MarkedValue {
            marker: self.marker,
            value: self.value.clone(),
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
