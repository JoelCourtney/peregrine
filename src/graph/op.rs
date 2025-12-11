use std::{
    mem::{take, transmute},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};

use parking_lot::Mutex;

use crate::{
    Callback, Ctx, Data, Downstream, Upstream,
    cache::{
        Cache, CheckResult,
        collector::{CollectionStatus, UpstreamCollector},
    },
    flow::Callbacks,
    node::Node,
};

use super::{NodeId, NodeTracker};

pub struct Op<UC: UpstreamCollector, O: 'static, F> {
    node: NodeTracker,
    upstreams: UC,
    collection_cells: Arc<UC::Cells>,
    counter: AtomicU32,
    func: F,
    callbacks: Mutex<Callbacks<O>>,
    cache: Arc<Cache<O>>,
    can_be_revalidated: AtomicBool,
}

type OpInput<UC> = <UC as UpstreamCollector>::Result;

impl<UC: UpstreamCollector, O: Data, F: Fn(OpInput<UC>) -> O + Send + Sync> Op<UC, O, F> {
    pub fn new(upstreams: UC, func: F, node_ids: impl IntoIterator<Item = NodeId>) -> Node<Self> {
        let node = NodeTracker::new();
        node.add_edges(node_ids);
        Node(Op {
            collection_cells: Arc::new(upstreams.new_cells()),
            upstreams,
            counter: AtomicU32::new(0),
            func,
            callbacks: Default::default(),
            cache: Cache::new_arc(),
            node,
            can_be_revalidated: AtomicBool::new(false),
        })
    }
}

impl<UC: UpstreamCollector, O: Data, F: Fn(OpInput<UC>) -> O + Send + Sync> Upstream
    for Op<UC, O, F>
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
                    CheckResult::YourProblem { .. } => unreachable!(),
                }
            }
            CheckResult::YourProblem { can_be_revalidated } => {
                self.can_be_revalidated
                    .store(can_be_revalidated, Ordering::Relaxed);
                self.callbacks.lock().add(callback, ctx.run_count);
                self.upstreams
                    .request(ctx, &self.collection_cells, &self.counter, unsafe {
                        transmute::<&dyn Downstream, &'static dyn Downstream>(self)
                    });
            }
        }
    }
}

impl<UC: UpstreamCollector, O: Data, F: Fn(OpInput<UC>) -> O + Send + Sync> Downstream
    for Op<UC, O, F>
{
    fn should_run(&self) -> bool {
        let counter = self
            .counter
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        debug_assert_ne!(counter, 0);
        counter == 1
    }

    fn run(&self, ctx: crate::Ctx) {
        let (inputs, collection_status) = self
            .upstreams
            .get(&self.collection_cells, || self.cache.get_invalidator());
        let result_factory = match collection_status {
            CollectionStatus::Revalidated if self.can_be_revalidated.load(Ordering::Relaxed) => {
                self.cache.revalidate()
            }
            _ => {
                let result = (self.func)(inputs);
                self.cache.resolve(
                    result,
                    matches!(collection_status, CollectionStatus::Constant),
                )
            }
        };
        let callbacks = take(&mut *self.callbacks.lock());
        callbacks.run(ctx, result_factory);
    }
}
