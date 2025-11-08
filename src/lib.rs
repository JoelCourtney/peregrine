pub mod cache;
pub mod data;
pub mod flow;
pub mod graph;
pub mod macro_prelude;

use std::sync::atomic::AtomicU64;

use data::Data;
use graph::NodeId;
pub use peregrine_macros::op;

use cache::Cached;
use forte::{Scope, ThreadPool};

use crate::{cache::collector::OutputCell, flow::Sink};

pub trait Upstream: Send + Sync {
    type Output: Data;

    fn node_id(&self) -> Option<NodeId>;
    fn request<'s>(&self, ctx: Ctx<'_, 's>, callback: Callback<'s, Self::Output>)
    where
        Self: 's;
}

pub struct Callback<'s, I> {
    downstream: &'s dyn Downstream,
    output: Box<dyn CallbackOutput<I> + 's>,
}

impl<'s, I> Callback<'s, I> {
    fn new(downstream: &'s dyn Downstream, output: impl CallbackOutput<I> + 's) -> Self {
        Callback {
            downstream,
            output: Box::new(output),
        }
    }
}

trait CallbackOutput<O>: Send {
    fn store(self: Box<Self>, value: Cached<O>);
}

impl<O: Send + 'static> CallbackOutput<O> for &OutputCell<O> {
    fn store(self: Box<Self>, value: Cached<O>) {
        (*self).store(value);
    }
}

struct CallbackMap<'s, I, O: 'static> {
    modification: Box<dyn FnOnce(Cached<I>) -> Cached<O> + Send>,
    and_then: Box<dyn CallbackOutput<O> + 's>,
}

impl<I, O: 'static> CallbackOutput<I> for CallbackMap<'_, I, O> {
    fn store(self: Box<Self>, value: Cached<I>) {
        let mapped = (self.modification)(value);
        self.and_then.store(mapped);
    }
}

impl<'s, O: 'static> Callback<'s, O> {
    pub fn call(self, value: Cached<O>, ctx: Ctx) {
        self.output.store(value);
        if self.downstream.should_run() {
            self.downstream.run(ctx);
        }
    }

    pub fn map<I: 's>(
        self,
        func: impl FnOnce(Cached<I>) -> Cached<O> + Send + 'static,
    ) -> Callback<'s, I> {
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
    pub run_count: u64,
}

pub trait IntoUpstream<U: Upstream> {
    fn into_upstream(self) -> U;
}

pub fn run<U: Upstream>(u: impl IntoUpstream<U>) -> U::Output {
    static COMPUTE: ThreadPool = ThreadPool::new();
    static RUN_COUNT: AtomicU64 = AtomicU64::new(0);

    COMPUTE.resize_to_available();

    let sink = Sink::new();

    let upstream = u.into_upstream();
    let run_count = RUN_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    COMPUTE.scope(|scope| {
        let ctx = Ctx { scope, run_count };
        upstream.request(ctx, sink.as_callback());
    });

    sink.open()
}
