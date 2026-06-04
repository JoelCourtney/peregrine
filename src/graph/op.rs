use std::{
    mem::take,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};

use parking_lot::Mutex;

use crate::{
    Callback, Ctx, Data, Downstream, Upstream, cache::{
        Cache, CheckResult,
        collector::{CollectionStatus, UpstreamCollector},
    }, callback::CallbackId, node::Node
};

pub struct Op<UC: UpstreamCollector, O: 'static, F> {
    upstreams: UC,
    collection_cells: Arc<UC::Cells>,
    counter: AtomicU32,
    func: F,
    callbacks: Mutex<Vec<CallbackId>>,
    cache: Arc<Cache<O>>,
    can_be_revalidated: AtomicBool,
}

type OpInput<UC> = <UC as UpstreamCollector>::Result;

impl<UC: UpstreamCollector, O: Data, F: Fn(OpInput<UC>) -> O + Send + Sync> Op<UC, O, F> {
    pub fn new(upstreams: UC, func: F) -> Node<Self> {
        Node(Op {
            collection_cells: Arc::new(upstreams.new_cells()),
            upstreams,
            counter: AtomicU32::new(0),
            func,
            callbacks: Mutex::default(),
            cache: Cache::new_arc(),
            can_be_revalidated: AtomicBool::new(false),
        })
    }
}

impl<UC: UpstreamCollector> Op<UC, UC::Result, fn(UC::Result) -> UC::Result>
where
    UC::Result: Data,
{
    pub fn collect(upstreams: UC) -> Node<Self> {
        Op::new(upstreams, identity)
    }
}

fn identity<T>(value: T) -> T {
    value
}

impl<UC: UpstreamCollector, O: Data, F: Fn(OpInput<UC>) -> O + Send + Sync> Upstream
    for Op<UC, O, F>
{
    type Output = O;

    fn request<'s>(&'s self, ctx: Ctx<'_, '_, 's>, callback: Callback<'s, Self::Output>)
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
                        callbacks.push(ctx.insert_callback(callback));
                    }
                    CheckResult::YourProblem { .. } => unreachable!(),
                }
            }
            CheckResult::YourProblem { can_be_revalidated } => {
                self.can_be_revalidated
                    .store(can_be_revalidated, Ordering::Relaxed);
                self.callbacks.lock().push(ctx.insert_callback(callback));
                self.upstreams
                    .request(ctx, &self.collection_cells, &self.counter, self);
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
        let (inputs, collection_status) =
            UC::get(&self.collection_cells, || self.cache.get_invalidator());
        let mut result_factory = match collection_status {
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
        let ids = take(&mut *self.callbacks.lock());
        if ids.is_empty() {
            return;
        }

        let callbacks = ids.into_iter().map(|id| ctx.take_callback::<O>(id));

        let mut to_run = callbacks.filter_map(|c| {
            c.output.store(result_factory());
            if c.downstream.should_run() {
                Some(c.downstream)
            } else {
                None
            }
        });

        let first = to_run.next();

        for downstream in to_run {
            ctx.spawn(move |ctx| {
                downstream.run(ctx);
            });
        }
        if let Some(downstream) = first {
            ctx.run(move |ctx| {
                downstream.run(ctx);
            });
        }
    }
}
