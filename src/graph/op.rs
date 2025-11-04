use std::{
    mem::{take, transmute},
    sync::{Arc, atomic::AtomicU32},
};

use parking_lot::Mutex;

use crate::{
    Callback, Ctx, Downstream, Upstream,
    cache::{Cache, CheckResult},
    flow::{Callbacks, UpstreamCollector, UpstreamCollectorExt},
};

pub struct Op<U, C, O: 'static, F> {
    collector: UpstreamCollector<U, C>,
    counter: AtomicU32,
    func: F,
    callbacks: Mutex<Callbacks<O>>,
    cache: Arc<Cache<O>>,
}

type OpInput<U, C> = <UpstreamCollector<U, C> as UpstreamCollectorExt<U>>::Combined;

impl<U: Send + Sync, C: Send + Sync, O: Send + Clone, F: Fn(OpInput<U, C>) -> O + Send + Sync>
    Op<U, C, O, F>
where
    UpstreamCollector<U, C>: UpstreamCollectorExt<U>,
{
    pub fn new(upstreams: U, func: F) -> Self {
        Op {
            collector: UpstreamCollector::new(upstreams),
            counter: AtomicU32::new(0),
            func,
            callbacks: Default::default(),
            cache: Cache::new_arc(),
        }
    }
}

impl<U: Send + Sync, C: Send + Sync, O: Send + Clone, F: Fn(OpInput<U, C>) -> O + Send + Sync>
    Upstream for Op<U, C, O, F>
where
    UpstreamCollector<U, C>: UpstreamCollectorExt<U>,
{
    type Output = O;

    fn request(&self, ctx: Ctx, callback: Callback<Self::Output>) {
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
                        callbacks.add(callback);
                    }
                    CheckResult::YourProblem => unreachable!(),
                }
            }
            CheckResult::YourProblem => {
                self.callbacks.lock().add(callback);
                self.collector.request(ctx, &self.counter, unsafe {
                    transmute::<&dyn Downstream, &'static dyn Downstream>(self)
                });
            }
        }
    }
}

impl<U: Send + Sync, C: Send + Sync, O: Send + Clone, F: Fn(OpInput<U, C>) -> O + Send + Sync>
    Downstream for Op<U, C, O, F>
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
