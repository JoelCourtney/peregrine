use std::{
    mem::{take, transmute},
    sync::{Arc, atomic::AtomicU32},
};

use parking_lot::Mutex;

use crate::{
    Callback, Ctx, Downstream, Upstream,
    cache::{Cache, CheckResult},
    data::Data,
    flow::{Callbacks, UpstreamCollector, UpstreamCollectorExt},
};

use super::{Node, NodeId};

pub struct Op<U, C, O: 'static, F> {
    node: Node,
    collector: UpstreamCollector<U, C>,
    counter: AtomicU32,
    func: F,
    callbacks: Mutex<Callbacks<O>>,
    cache: Arc<Cache<O>>,
}

type OpInput<U, C> = <UpstreamCollector<U, C> as UpstreamCollectorExt<U>>::Combined;

impl<U: Send + Sync, C: Send + Sync, O: Data, F: Fn(OpInput<U, C>) -> O + Send + Sync>
    Op<U, C, O, F>
where
    UpstreamCollector<U, C>: UpstreamCollectorExt<U>,
{
    pub fn new(upstreams: U, func: F, node_ids: impl IntoIterator<Item = NodeId>) -> Self {
        let node = Node::new();
        node.add_edges(node_ids);
        Op {
            collector: UpstreamCollector::new(upstreams),
            counter: AtomicU32::new(0),
            func,
            callbacks: Default::default(),
            cache: Cache::new_arc(),
            node,
        }
    }
}

impl<U: Send + Sync, C: Send + Sync, O: Data, F: Fn(OpInput<U, C>) -> O + Send + Sync> Upstream
    for Op<U, C, O, F>
where
    UpstreamCollector<U, C>: UpstreamCollectorExt<U>,
{
    type Output = O;

    fn node_id(&self) -> Option<super::NodeId> {
        Some(self.node.id)
    }
    fn request<'s>(&self, ctx: Ctx<'_, 's>, callback: Callback<Self::Output>)
    where
        Self: 's,
    {
        match self.cache.check() {
            CheckResult::NoProblem(r) => {
                callback.call(r, ctx);
            }
            CheckResult::SomeoneElsesProblem => {
                let mut callbacks = self.callbacks.lock();

                // Hold the callbacks lock and check again.
                // Without double checking, its possible the
                // thread in charge of this node could have
                // finished and locked the callbacks in
                // between this thread's check and callback
                // acquisition, leaving this callback unresolved.
                match self.cache.check() {
                    CheckResult::NoProblem(r) => {
                        drop(callbacks);
                        callback.call(r, ctx);
                    }
                    CheckResult::SomeoneElsesProblem => {
                        callbacks.add(callback, ctx.run_count);
                    }
                    CheckResult::YourProblem => unreachable!(),
                }
            }
            CheckResult::YourProblem => {
                self.callbacks.lock().add(callback, ctx.run_count);
                self.collector.request(ctx, &self.counter, unsafe {
                    transmute::<&dyn Downstream, &'static dyn Downstream>(self)
                });
            }
        }
    }
}

impl<U: Send + Sync, C: Send + Sync, O: Data, F: Fn(OpInput<U, C>) -> O + Send + Sync> Downstream
    for Op<U, C, O, F>
where
    UpstreamCollector<U, C>: UpstreamCollectorExt<U>,
{
    fn should_run(&self) -> bool {
        let counter = self
            .counter
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        debug_assert_ne!(counter, 0);
        counter == 1
    }

    fn run(&self, ctx: crate::Ctx) {
        let inputs = self.collector.get();
        let result_factory = self.cache.resolve(inputs, &self.func);
        let callbacks = take(&mut *self.callbacks.lock());
        callbacks.run(ctx, result_factory);
    }
}
