#![doc(hidden)]

pub mod grounding;
pub mod initial_conditions;
pub mod node_impls;

use crate::internal::exec::ExecEnvironment;
use crate::internal::timeline::Timelines;
use crate::public::resource::Data;
use crate::public::resource::Resource;
use anyhow::Result;
use derive_more::with_trait::Error as DeriveError;
use grounding::GroundingContinuation;
use grounding::peregrine_grounding;
use hifitime::Duration;
use rayon::Scope;
use smallvec::SmallVec;
use std::fmt::{Debug, Display, Formatter};

pub type InternalResult<T> = Result<T, ObservedErrorOutput>;

pub trait Node<'o>: Sync {
    fn insert_self(&'o self, timelines: &Timelines<'o>, is_daemon: bool) -> Result<()>;
    fn remove_self(&self, timelines: &Timelines<'o>, is_daemon: bool) -> Result<()>;
}

pub trait NodeId {
    const ID: u64;
}

pub trait Downstream<'o, R: Resource>: Sync + GroundingDownstream<'o> {
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

pub trait GroundingDownstream<'o>: Sync {
    fn respond_grounding<'s>(
        &'o self,
        value: InternalResult<(usize, Duration)>,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's;

    fn clear_grounding_cache(&self);
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

    fn request_grounding<'s>(
        &'o self,
        continuation: GroundingContinuation<'o>,
        already_registered: bool,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's;
}

pub enum Continuation<'o, R: Resource> {
    Node(&'o dyn Downstream<'o, R>),
    Root(oneshot::Sender<InternalResult<<R::Data as Data<'o>>::Read>>),
    GroundingWrapper(GroundingContinuation<'o>),
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
            Continuation::GroundingWrapper(c) => {
                if castaway::cast!(R::INSTANCE, peregrine_grounding).is_ok() {
                    assert_eq!(
                        std::mem::size_of::<InternalResult<(u64, Duration)>>(),
                        std::mem::size_of::<InternalResult<(u64, <R::Data as Data<'o>>::Read)>>()
                    );
                    let v: InternalResult<(u64, Duration)> =
                        unsafe { std::mem::transmute_copy(&value) };
                    c.run(v.map(|(_, d)| d), scope, timelines, env);
                } else {
                    unreachable!()
                }
            }
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
