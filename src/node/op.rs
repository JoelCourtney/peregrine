use std::sync::Arc;

use crate::{
    Ctx, IntoRun, Run, RunInWorld,
    cache::{Cache, InvalidatorGenerator, MaybeCached},
    world::World,
};

pub struct Op<O: Send, F: Fn(Ctx, InvalidatorGenerator<O>) -> O> {
    world: World,
    runner: OpRunner<O, F>,
}

pub struct OpRunner<O: Send, F: Fn(Ctx, InvalidatorGenerator<O>) -> O> {
    f: F,
    cache: Arc<Cache<O>>,
}

impl<O: Send, F: Fn(Ctx, InvalidatorGenerator<O>) -> O> Op<O, F> {
    pub fn new(world: World, f: F) -> Self {
        Op {
            world,
            runner: OpRunner {
                f,
                cache: Cache::new_arc(),
            },
        }
    }
}

impl<O: Send + Clone + 'static, F: Fn(Ctx, InvalidatorGenerator<O>) -> O + Send + Sync>
    IntoRun<OpRunner<O, F>> for Op<O, F>
{
    fn into_run(self) -> RunInWorld<OpRunner<O, F>> {
        RunInWorld {
            run: self.runner,
            world: self.world,
        }
    }
}

impl<'a, O: Send + Clone + 'static, F: Fn(Ctx, InvalidatorGenerator<O>) -> O + Send + Sync>
    IntoRun<&'a OpRunner<O, F>> for &'a Op<O, F>
{
    fn into_run(self) -> RunInWorld<&'a OpRunner<O, F>> {
        RunInWorld {
            run: &self.runner,
            world: self.world.clone(),
        }
    }
}

impl<O: Send + Clone + 'static, F: Fn(Ctx, InvalidatorGenerator<O>) -> O + Send + Sync> Run
    for OpRunner<O, F>
{
    type Output = O;

    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        self.cache.resolve(ctx.worker, |g| (self.f)(ctx, g), false)
    }
}
