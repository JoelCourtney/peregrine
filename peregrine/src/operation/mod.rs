#![doc(hidden)]

pub mod grounding;
pub mod initial_conditions;

use crate as peregrine;
use crate::exec::ExecEnvironment;
use crate::macro_prelude::{Data, MaybeHash};
use crate::resource::Resource;
use crate::timeline::Timelines;
use crate::{Model, Time};
use anyhow::Result;
use derive_more::with_trait::Error as DeriveError;
use hifitime::Duration;
use rayon::Scope;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::fmt::{Debug, Display, Formatter};
use std::hash::Hasher;
use std::marker::PhantomData;

pub type InternalResult<T> = Result<T, ObservedErrorOutput>;

pub trait Node<'o, M: Model<'o> + 'o>: Sync {
    fn insert_self(&'o self, timelines: &mut Timelines<'o, M>) -> Result<()>;
    fn remove_self(&self, timelines: &mut Timelines<'o, M>) -> Result<()>;
}

pub trait NodeId {
    const ID: u64;
}

pub trait Downstream<'o, R: Resource, M: Model<'o> + 'o>: Sync {
    fn respond<'s>(
        &'o self,
        value: InternalResult<(u64, <R::Data as Data<'o>>::Read)>,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o, M>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's;

    fn clear_cache(&self);
    fn clear_upstream(&self, time_of_change: Option<Duration>) -> bool;
}

pub trait Upstream<'o, R: Resource, M: Model<'o> + 'o>: Sync {
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

pub enum Continuation<'o, R: Resource, M: Model<'o> + 'o> {
    Node(&'o dyn Downstream<'o, R, M>),
    MarkedNode(usize, &'o dyn Downstream<'o, Marked<R>, M>),
    Root(oneshot::Sender<InternalResult<<R::Data as Data<'o>>::Read>>),
}

impl<'o, R: Resource, M: Model<'o> + 'o> Continuation<'o, R, M> {
    pub fn run<'s>(
        self,
        value: InternalResult<(u64, <R::Data as Data<'o>>::Read)>,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o, M>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's,
    {
        match self {
            Continuation::Node(n) => n.respond(value, scope, timelines, env),
            Continuation::MarkedNode(marker, n) => n.respond(
                value.map(|(hash, value)| (hash, (marker, value))),
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
pub enum Marked<R: Resource> {
    Unit,
    Phantom(PhantomData<R>),
}

impl<R: Resource> Resource for Marked<R> {
    const LABEL: &'static str = R::LABEL;
    const ID: u64 = peregrine_macros::random_u64!();
    type Data = MarkedValue<R>;
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MarkedValue<R: Resource> {
    pub(crate) marker: usize,
    pub(crate) value: R::Data,
}

impl<R: Resource> MaybeHash for MarkedValue<R> {
    fn is_hashable(&self) -> bool {
        self.value.is_hashable()
    }
    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.value.hash_unchecked(state);
    }
}

impl<'h, R: Resource> Data<'h> for MarkedValue<R> {
    type Read = (usize, <R::Data as Data<'h>>::Read);
    type Sample = <R::Data as Data<'h>>::Sample;

    fn to_read(&self, written: Time) -> Self::Read {
        (self.marker, self.value.to_read(written))
    }
    fn from_read(read: Self::Read, now: Time) -> Self {
        MarkedValue {
            marker: read.0,
            value: R::Data::from_read(read.1, now),
        }
    }
    fn sample(read: &Self::Read, now: Time) -> Self::Sample {
        R::Data::sample(&read.1, now)
    }
}

impl<R: Resource> Copy for MarkedValue<R> where R::Data: Copy {}
impl<R: Resource> Clone for MarkedValue<R>
where
    R::Data: Clone,
{
    fn clone(&self) -> Self {
        MarkedValue {
            marker: self.marker,
            value: self.value.clone(),
        }
    }
}

pub enum MaybeMarkedDownstream<'o, R: Resource, M: Model<'o>> {
    Unmarked(&'o dyn Downstream<'o, R, M>),
    Marked(&'o dyn Downstream<'o, Marked<R>, M>),
}

impl<'o, R: Resource, M: Model<'o>> MaybeMarkedDownstream<'o, R, M> {
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

impl<'o, R: Resource, M: Model<'o>> From<&'o dyn Downstream<'o, R, M>>
    for MaybeMarkedDownstream<'o, R, M>
{
    fn from(value: &'o dyn Downstream<'o, R, M>) -> Self {
        MaybeMarkedDownstream::Unmarked(value)
    }
}

impl<'o, R: Resource, M: Model<'o>> From<&'o dyn Downstream<'o, Marked<R>, M>>
    for MaybeMarkedDownstream<'o, R, M>
{
    fn from(value: &'o dyn Downstream<'o, Marked<R>, M>) -> Self {
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
