pub mod cache;
pub mod data;
pub mod flow;
pub mod graph;
pub mod macro_prelude;
pub mod node;
mod shared_lock;
pub mod undo;

pub mod plan;
pub mod specification;

use std::sync::atomic::AtomicU64;

pub use peregrine_macros::{AutoSource, Chronological, Undo, action, activity, op, sync};

use cache::Cached;
use rayon::Scope;

use crate::{cache::collector::OutputCell, data::Data, flow::Sink, shared_lock::SharedLockKey};

pub trait Upstream: Send + Sync {
    type Output: Data;

    fn request<'s>(&self, ctx: Ctx<'_, '_, 's>, callback: Callback<'s, Self::Output>)
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
pub struct Ctx<'a, 'b, 's>
where
    'b: 's,
{
    scope: &'a Scope<'s>,
    run_count: u64,
    stack_depth: u32,
    key: &'b SharedLockKey<'s>,
}

const MAX_STACK_DEPTH: u32 = 1000;

impl<'a, 'b, 's> Ctx<'a, 'b, 's> {
    fn new(scope: &'a Scope<'s>, run_count: u64, key: &'b SharedLockKey<'s>) -> Self {
        Ctx {
            scope,
            stack_depth: 0,
            run_count,
            key,
        }
    }

    #[inline]
    pub fn spawn(&self, f: impl FnOnce(Ctx<'_, '_, 's>) + Send + 's) {
        let run_count = self.run_count;
        let key = self.key;
        self.scope
            .spawn(move |scope| f(Ctx::new(scope, run_count, key)));
    }

    #[inline]
    pub fn run(&self, f: impl FnOnce(Ctx<'_, '_, 's>) + Send + 's) {
        if self.stack_depth >= MAX_STACK_DEPTH {
            self.spawn(f);
        } else {
            f(Ctx {
                scope: self.scope,
                run_count: self.run_count,
                stack_depth: self.stack_depth + 1,
                key: self.key,
            });
        }
    }
}

pub fn run<O: Data>(upstream: impl Upstream<Output = O>) -> O {
    static RUN_COUNT: AtomicU64 = AtomicU64::new(0);

    let sink = Sink::new();

    let run_count = RUN_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let key = SharedLockKey::new();

    rayon::scope(|scope| {
        let ctx = Ctx::new(scope, run_count, &key);
        upstream.request(ctx, sink.as_callback());
    });

    sink.open()
}
