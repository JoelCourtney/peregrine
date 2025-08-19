use std::sync::Arc;

use forte::Worker;
use futures::future::Shared;

use crate::{
    IntoNode, Node,
    cache::{Cache, MaybeCached, UpstreamReceiver},
    data::Data,
};

impl<N: Node> IntoNode<N> for N {
    fn into_node(self) -> N {
        self
    }
}

impl<N: Node> Node for &N {
    type Output = N::Output;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        (**self).run(w)
    }
}

impl<N: Node + ?Sized> Node for Box<N> {
    type Output = N::Output;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        (**self).run(w)
    }
}

impl<N: Node> Node for Arc<N> {
    type Output = N::Output;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        (**self).run(w)
    }
}

#[derive(Debug)]
pub struct DataWrapper<O>(O);

impl<O: Data> Node for DataWrapper<O> {
    type Output = O;

    fn run(&self, _: &Worker) -> MaybeCached<Self::Output> {
        MaybeCached::Constant(self.0.clone())
    }
}

impl<O: Data> IntoNode<DataWrapper<O>> for O {
    fn into_node(self) -> DataWrapper<O> {
        DataWrapper(self)
    }
}

#[derive(Debug)]
pub struct FnWrapper<F>(F);

impl<O: Send, F: Fn() -> MaybeCached<O> + Send + Sync> Node for FnWrapper<F> {
    type Output = O;

    fn run(&self, _: &Worker) -> MaybeCached<Self::Output> {
        self.0()
    }
}

impl<O: Send, F: Fn() -> MaybeCached<O> + Send + Sync> IntoNode<FnWrapper<F>> for F {
    fn into_node(self) -> FnWrapper<F> {
        FnWrapper(self)
    }
}

#[derive(Debug)]
pub struct WorkerFnWrapper<F>(F);

impl<O: Send, F: Fn(&Worker) -> MaybeCached<O> + Send + Sync> Node for WorkerFnWrapper<F> {
    type Output = O;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        self.0(w)
    }
}

impl<O: Send, F: Fn(&Worker) -> MaybeCached<O> + Send + Sync> IntoNode<WorkerFnWrapper<F>> for F {
    fn into_node(self) -> WorkerFnWrapper<F> {
        WorkerFnWrapper(self)
    }
}

#[derive(Debug)]
pub struct AsyncWrapper<F: Future>(Shared<F>);

impl<O: Send + Sync + Clone, F: Future<Output = O> + Send + Sync> Node for AsyncWrapper<F> {
    type Output = O;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        MaybeCached::Constant(w.block_on(self.0.clone()))
    }
}

impl<O: Send + Sync + Clone, F: Future<Output = O> + Send + Sync> IntoNode<AsyncWrapper<F>> for F {
    fn into_node(self) -> AsyncWrapper<F> {
        use futures::future::FutureExt;
        AsyncWrapper(self.shared())
    }
}

pub struct CachedFnWrapper<O: Send, F: Fn(&Worker, &mut UpstreamReceiver<O>) -> O> {
    f: F,
    cache: Arc<Cache<O>>,
}

impl<O: Send + Clone + 'static, F: Fn(&Worker, &mut UpstreamReceiver<O>) -> O + Send + Sync>
    IntoNode<CachedFnWrapper<O, F>> for F
{
    fn into_node(self) -> CachedFnWrapper<O, F> {
        CachedFnWrapper {
            f: self,
            cache: Cache::new(),
        }
    }
}

impl<O: Send + Clone + 'static, F: Fn(&Worker, &mut UpstreamReceiver<O>) -> O + Send + Sync> Node
    for CachedFnWrapper<O, F>
{
    type Output = O;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        self.cache
            .try_resolve(|r| (self.f)(w, r))
            .unwrap_or_else(|| {
                w.block_on(self.cache.resolve(|r| {
                    Worker::with_current(|w| {
                        (self.f)(w.expect("Expected to be run on thread pool"), r)
                    })
                }))
            })
    }
}

pub struct TupleWrapper<T>(T);

macro_rules! impl_into_node_for_tuple {
    ($($t:ident $t_i:ident),*) => {
        peregrine_macros::impl_node_for_tuple_wrapper!($($t),*);
        impl<$($t: Node, $t_i: IntoNode<$t>),*> IntoNode<TupleWrapper<($($t,)*)>> for ($($t_i,)*) {
            #[allow(non_snake_case)]
            fn into_node(self) -> TupleWrapper<($($t,)*)> {
                let ($($t_i,)*) = self;
                TupleWrapper(($($t_i.into_node()),*))
            }
        }
    };
}

impl_into_node_for_tuple!(A AI, B BI, C CI);
impl_into_node_for_tuple!(A AI, B BI, C CI, D DI);
impl_into_node_for_tuple!(A AI, B BI, C CI, D DI, E EI);
impl_into_node_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI);
impl_into_node_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI);
impl_into_node_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI);
impl_into_node_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI, I II);
impl_into_node_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI, I II, J JI);
impl_into_node_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI, I II, J JI, K KI);
impl_into_node_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI, I II, J JI, K KI, L LI);

impl<A: Node, AI: IntoNode<A>> IntoNode<TupleWrapper<(A,)>> for (AI,) {
    fn into_node(self) -> TupleWrapper<(A,)> {
        TupleWrapper((self.0.into_node(),))
    }
}

impl<A: Node> Node for TupleWrapper<(A,)> {
    type Output = (A::Output,);

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        self.0.0.run(w).map(|v| (v,))
    }
}

#[cfg(test)]
mod tests {
    use crate::run;

    #[test]
    fn test_async() {
        let result = run(async { 5 });

        assert_eq!(result, 5);
    }
}
