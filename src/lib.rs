pub mod cache;
pub(crate) mod callback;
pub mod data;
pub mod graph;
mod lock;
pub mod macro_prelude;
pub mod node;
pub mod undo;

pub mod plan;
pub mod specification;

use std::sync::Arc;

use child_lock::parking_lot::MutexKey;
pub use peregrine_macros::{AutoSource, Chronological, Undo, action, activity, op};

use rayon::Scope;
use sharded_slab::Slab;

use crate::{
    callback::{Callback, CallbackId, ErasedCallback, Sink},
    data::Data,
    lock::PARENT,
};

pub trait Upstream: Send + Sync {
    type Output: Data;

    fn request<'s>(&'s self, ctx: Ctx<'_, '_, 's>, callback: Callback<'s, Self::Output>)
    where
        Self: 's;
}

pub trait Downstream: Send + Sync {
    fn should_run(&self) -> bool;
    fn run(&self, ctx: Ctx);
}

pub struct Ctx<'a, 'b, 's>
where
    'b: 's,
{
    scope: &'a Scope<'s>,
    stack_depth: u32,
    key: &'b MutexKey<'s>,
    callbacks: Arc<Slab<ErasedCallback<'s>>>,
}

const MAX_STACK_DEPTH: u32 = 1000;

impl<'a, 'b, 's> Ctx<'a, 'b, 's> {
    fn new(
        scope: &'a Scope<'s>,
        key: &'b MutexKey<'s>,
        callbacks: Arc<Slab<ErasedCallback<'s>>>,
    ) -> Self {
        Ctx {
            scope,
            stack_depth: 0,
            key,
            callbacks,
        }
    }

    #[inline]
    fn spawn(&self, f: impl FnOnce(Ctx<'_, '_, 's>) + Send + 's) {
        let key = self.key;
        let callbacks = self.callbacks.clone();
        self.scope
            .spawn(move |scope| f(Ctx::new(scope, key, callbacks)));
    }

    #[inline]
    fn run(&self, f: impl FnOnce(Ctx<'_, '_, 's>) + Send + 's) {
        if self.stack_depth >= MAX_STACK_DEPTH {
            self.spawn(f);
        } else {
            f(Ctx {
                scope: self.scope,
                stack_depth: self.stack_depth + 1,
                key: self.key,
                callbacks: self.callbacks.clone(),
            });
        }
    }

    fn insert_callback<O: 'static>(&self, callback: Callback<'s, O>) -> CallbackId {
        CallbackId(
            self.callbacks
                .insert(ErasedCallback::new(callback))
                .expect("Could not allocate in slab"),
        )
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "Callback ids should be dropped so they are not reused"
    )]
    fn take_callback<O: 'static>(&self, id: CallbackId) -> Callback<'s, O> {
        self.callbacks
            .take(id.0)
            .expect("Callback not found")
            .open()
    }
}

pub fn run<O: Data>(upstream: impl Upstream<Output = O>) -> O {
    let sink = Sink::new();

    let key = PARENT.key();

    rayon::scope(|scope| {
        let ctx = Ctx::new(scope, &key, Arc::new(Slab::new()));
        upstream.request(ctx, sink.as_callback());
    });

    sink.open()
}
