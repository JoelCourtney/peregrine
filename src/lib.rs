pub mod cache;
pub mod data;
pub mod flow;
pub mod graph;
pub mod macro_prelude;

use crossbeam::atomic::AtomicCell;
use graph::NodeId;
pub use peregrine_macros::op;

use cache::Cached;
use forte::{Scope, ThreadPool};

use crate::{flow::Sink, graph::GRAPH};

pub trait Upstream: Send + Sync {
    type Output: Send + 'static;

    fn node_id(&self) -> Option<NodeId>;
    fn request(&self, ctx: Ctx, callback: Callback<Self::Output>);
}

pub struct Callback<I: 'static> {
    downstream: &'static dyn Downstream,
    output: Box<dyn CallbackOutput<I>>,
}

impl<I: 'static> Callback<I> {
    fn new(downstream: &'static dyn Downstream, output: impl CallbackOutput<I> + 'static) -> Self {
        Callback {
            downstream,
            output: Box::new(output),
        }
    }
}

trait CallbackOutput<O>: Send {
    fn store(self: Box<Self>, value: Cached<O>);
}

impl<O: Send + 'static> CallbackOutput<O> for &'static AtomicCell<Option<Cached<O>>> {
    fn store(self: Box<Self>, value: Cached<O>) {
        AtomicCell::store(&self, Some(value));
    }
}

struct CallbackMap<I, O: 'static> {
    modification: Box<dyn FnOnce(Cached<I>) -> Cached<O> + Send>,
    and_then: Box<dyn CallbackOutput<O>>,
}

impl<I, O: 'static> CallbackOutput<I> for CallbackMap<I, O> {
    fn store(self: Box<Self>, value: Cached<I>) {
        let mapped = (self.modification)(value);
        self.and_then.store(mapped);
    }
}

impl<O> Callback<O> {
    pub fn call(self, value: Cached<O>, ctx: Ctx) {
        self.output.store(value);
        if self.downstream.should_run() {
            self.downstream.run(ctx);
        }
    }

    pub fn map<I>(self, func: impl FnOnce(Cached<I>) -> Cached<O> + Send + 'static) -> Callback<I> {
        Callback {
            output: Box::new(CallbackMap {
                modification: Box::new(func),
                and_then: self.output,
            }),
            downstream: self.downstream,
        }
    }
}

pub trait Downstream: Send + Sync {
    fn should_run(&self) -> bool;
    fn run(&self, ctx: Ctx);
}

#[derive(Copy, Clone)]
pub struct Ctx<'a, 's> {
    pub scope: &'a Scope<'s>,
}

pub trait IntoUpstream<U: Upstream> {
    fn into_upstream(self) -> U;
}

pub fn run<U: Upstream>(u: impl IntoUpstream<U>) -> U::Output {
    static COMPUTE: ThreadPool = ThreadPool::new();

    COMPUTE.resize_to_available();

    let sink = Sink::new();

    let upstream = u.into_upstream();

    let _graph = GRAPH.lock();
    COMPUTE.scope(|scope| {
        let ctx = Ctx { scope };
        upstream.request(ctx, sink.as_callback());
    });

    sink.open()
}
