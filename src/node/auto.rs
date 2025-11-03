use std::sync::Arc;

use futures::future::Shared;

use crate::{
    Ctx, IntoRun, Run, RunInWorld,
    cache::{Cache, MaybeCached},
    data::Data,
    world::World,
};

impl<R: Run + ?Sized> Run for &R {
    type Output = R::Output;

    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        (**self).run(ctx)
    }
}

impl<R: Run + ?Sized> Run for Box<R> {
    type Output = R::Output;

    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        (**self).run(ctx)
    }
}

impl<R: Run + ?Sized> Run for Arc<R> {
    type Output = R::Output;

    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        (**self).run(ctx)
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct DataWrapper<O>(O);

impl<O: Data> Run for DataWrapper<O> {
    type Output = O;

    #[inline(always)]
    fn run(&self, _: Ctx) -> MaybeCached<Self::Output> {
        MaybeCached::Constant(self.0.clone())
    }
}

impl<O: Data> IntoRun<DataWrapper<O>> for O {
    fn into_run(self) -> RunInWorld<DataWrapper<O>> {
        RunInWorld::new(DataWrapper(self), World::new())
    }
}

#[derive(Debug)]
pub struct FnWrapper<F>(F);

impl<O: Send + 'static, F: Fn() -> MaybeCached<O> + Send + Sync> Run for FnWrapper<F> {
    type Output = O;

    #[inline(always)]
    fn run(&self, _: Ctx) -> MaybeCached<Self::Output> {
        self.0()
    }
}

impl<O: Send + 'static, F: Fn() -> MaybeCached<O> + Send + Sync> IntoRun<FnWrapper<F>> for F {
    fn into_run(self) -> RunInWorld<FnWrapper<F>> {
        RunInWorld::new(FnWrapper(self), World::new())
    }
}

#[derive(Debug)]
pub struct AsyncWrapper<F: Future>(Shared<F>);

impl<O: Send + Sync + Clone + 'static, F: Future<Output = O> + Send + Sync> Run
    for AsyncWrapper<F>
{
    type Output = O;

    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        MaybeCached::Constant(ctx.worker.block_on(self.0.clone()))
    }
}

impl<O: Send + Sync + Clone + 'static, F: Future<Output = O> + Send + Sync> IntoRun<AsyncWrapper<F>>
    for F
{
    fn into_run(self) -> RunInWorld<AsyncWrapper<F>> {
        use futures::future::FutureExt;
        RunInWorld::new(AsyncWrapper(self.shared()), World::new())
    }
}

pub struct TupleWrapper<T, C>(T, Arc<Cache<C>>);

macro_rules! impl_into_run_for_tuple {
    ($($t:ident $t_i:ident),*) => {
        peregrine_macros::impl_op_for_tuple_wrapper!($($t),*);
        impl<$($t: Run, $t_i: IntoRun<$t>),*> IntoRun<TupleWrapper<($($t,)*), ($($t::Output,)*)>> for ($($t_i,)*) where $($t::Output: Clone + 'static),* {
            #[allow(non_snake_case)]
            fn into_run(self) -> RunInWorld<TupleWrapper<($($t,)*), ($($t::Output,)*)>> {
                let ($($t_i,)*) = self;

                let ($($t_i,)*) = ($($t_i.into_run()),*);

                let mut world = World::new();
                $(
                    world = world.merge($t_i.world).expect("Cannot merge nodes from different worlds into the same tuple");
                )*
                RunInWorld::new(TupleWrapper(($($t_i.run,)*), Cache::new_arc()), world)
            }
        }
    };
}

impl_into_run_for_tuple!(A AI, B BI, C CI);
impl_into_run_for_tuple!(A AI, B BI, C CI, D DI);
impl_into_run_for_tuple!(A AI, B BI, C CI, D DI, E EI);
impl_into_run_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI);
impl_into_run_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI);
impl_into_run_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI);
impl_into_run_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI, I II);
impl_into_run_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI, I II, J JI);
impl_into_run_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI, I II, J JI, K KI);
impl_into_run_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI, I II, J JI, K KI, L LI);

pub struct UnaryTupleWrapper<A: Run>(A);

impl<A: Run, AI: IntoRun<A>> IntoRun<UnaryTupleWrapper<A>> for (AI,) {
    fn into_run(self) -> RunInWorld<UnaryTupleWrapper<A>> {
        let RunInWorld { run, world } = self.0.into_run();
        RunInWorld::new(UnaryTupleWrapper(run), world)
    }
}

impl<A: Run> Run for UnaryTupleWrapper<A> {
    type Output = (A::Output,);

    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        self.0.run(ctx).map(|v| (v,))
    }
}

impl<R: Run> IntoRun<Self> for Option<R> {
    fn into_run(self) -> RunInWorld<Self> {
        RunInWorld::new(self, World::new())
    }
}

impl<R: Run> Run for Option<R> {
    type Output = Option<R::Output>;

    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        match self {
            Some(r) => r.run(ctx).map(Some),
            None => MaybeCached::Constant(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::run;

    #[test]
    fn test_async() {
        let result = run(async { 5 });

        assert_eq!(result, Ok(5));
    }
}
