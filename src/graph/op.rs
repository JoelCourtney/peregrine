use std::sync::Arc;

use crate::{
    Ctx, Run,
    cache::{Cache, InvalidatorGenerator, MaybeCached},
};

pub struct Op<O: Send, F: Fn(Ctx, InvalidatorGenerator<O>) -> O> {
    f: F,
    cache: Arc<Cache<O>>,
}

impl<O: Send, F: Fn(Ctx, InvalidatorGenerator<O>) -> O> Op<O, F> {
    pub fn new(f: F) -> Self {
        Op {
            f,
            cache: Cache::new_arc(),
        }
    }
}

impl<O: Send + Clone + 'static, F: Fn(Ctx, InvalidatorGenerator<O>) -> O + Send + Sync> Run
    for Op<O, F>
{
    type Output = O;

    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        self.cache.resolve(ctx.worker, |g| (self.f)(ctx, g), false)
    }
}
