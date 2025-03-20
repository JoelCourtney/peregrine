#![doc(hidden)]

pub mod grounding;
pub mod initial_conditions;
pub mod node_impls;

use crate::exec::ExecEnvironment;
use crate::macro_prelude::Data;
use crate::resource::Resource;
use crate::timeline::Timelines;
use anyhow::Result;
use derive_more::with_trait::Error as DeriveError;
use hifitime::Duration;
use rayon::Scope;
use smallvec::SmallVec;
use std::fmt::{Debug, Display, Formatter};

pub type InternalResult<T> = Result<T, ObservedErrorOutput>;

pub trait Node<'o>: Sync {
    fn insert_self<'s>(&'o self, timelines: &'s Timelines<'o>, scope: &Scope<'s>) -> Result<()>
    where
        'o: 's;
    fn remove_self(&self, timelines: &Timelines<'o>) -> Result<()>;
}

pub trait NodeId {
    const ID: u64;
}

pub trait Downstream<'o, R: Resource>: Sync {
    fn respond<'s>(
        &'o self,
        value: InternalResult<(u64, <R::Data as Data<'o>>::Read)>,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's;

    fn clear_cache(&self);
    fn clear_upstream(&self, time_of_change: Option<Duration>) -> bool;
}

pub trait Upstream<'o, R: Resource>: Sync {
    fn request<'s>(
        &'o self,
        continuation: Continuation<'o, R>,
        already_registered: bool,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's;

    fn notify_downstreams(&self, time_of_change: Duration);
    fn register_downstream_early(&self, downstream: &'o dyn Downstream<'o, R>);
}

pub enum Continuation<'o, R: Resource> {
    Node(&'o dyn Downstream<'o, R>),
    Root(oneshot::Sender<InternalResult<<R::Data as Data<'o>>::Read>>),
}

impl<'o, R: Resource> Continuation<'o, R> {
    pub fn run<'s>(
        self,
        value: InternalResult<(u64, <R::Data as Data<'o>>::Read)>,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's,
    {
        match self {
            Continuation::Node(n) => n.respond(value, scope, timelines, env),
            Continuation::Root(s) => s.send(value.map(|r| r.1)).unwrap(),
        }
    }

    pub fn copy_node(&self) -> Option<Self> {
        match &self {
            Continuation::Node(n) => Some(Continuation::Node(*n)),
            _ => None,
        }
    }

    pub fn to_downstream(&self) -> Option<&'o dyn Downstream<'o, R>> {
        match self {
            Continuation::Node(n) => Some(*n),
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

pub type UpstreamVec<'o, R> = SmallVec<&'o dyn Upstream<'o, R>, 2>;
