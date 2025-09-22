use std::sync::Arc;

use forte::Worker;
use futures::future::Shared;

use crate::{
    IntoNode, Node,
    cache::{Cache, InvalidatorGenerator, MaybeCached},
    data::Data,
};

pub struct NodeBox<O>(Box<dyn Node<Output = O>>);

impl<O> NodeBox<O> {
    pub fn new<N: Node<Output = O> + 'static>(node: impl IntoNode<N>) -> Self {
        Self(Box::new(node.into_node()))
    }
}

impl<O: Send + 'static> Node for NodeBox<O> {
    type Output = O;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        self.0.run(w)
    }
}

impl<O: Send + 'static> IntoNode<NodeBox<O>> for Box<dyn Node<Output = O>> {
    fn into_node(self) -> NodeBox<O> {
        NodeBox(self)
    }
}

pub struct NodeArc<O>(Arc<dyn Node<Output = O>>);

impl<O> NodeArc<O> {
    pub fn new<N: Node<Output = O> + 'static>(node: impl IntoNode<N>) -> Self {
        Self(Arc::new(node.into_node()))
    }
}

impl<O> Clone for NodeArc<O> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<O: Send + 'static> Node for NodeArc<O> {
    type Output = O;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        self.0.run(w)
    }
}

impl<O: Send + 'static> IntoNode<NodeArc<O>> for Arc<dyn Node<Output = O>> {
    fn into_node(self) -> NodeArc<O> {
        NodeArc(self)
    }
}

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
#[repr(transparent)]
pub struct DataWrapper<O>(O);

impl<O: Data> Node for DataWrapper<O> {
    type Output = O;

    #[inline(always)]
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

impl<O: Send + 'static, F: Fn() -> MaybeCached<O> + Send + Sync> Node for FnWrapper<F> {
    type Output = O;

    #[inline(always)]
    fn run(&self, _: &Worker) -> MaybeCached<Self::Output> {
        self.0()
    }
}

impl<O: Send + 'static, F: Fn() -> MaybeCached<O> + Send + Sync> IntoNode<FnWrapper<F>> for F {
    fn into_node(self) -> FnWrapper<F> {
        FnWrapper(self)
    }
}

#[derive(Debug)]
pub struct WorkerFnWrapper<F>(F);

impl<O: Send + 'static, F: Fn(&Worker) -> MaybeCached<O> + Send + Sync> Node
    for WorkerFnWrapper<F>
{
    type Output = O;

    #[inline(always)]
    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        self.0(w)
    }
}

impl<O: Send + 'static, F: Fn(&Worker) -> MaybeCached<O> + Send + Sync> IntoNode<WorkerFnWrapper<F>>
    for F
{
    fn into_node(self) -> WorkerFnWrapper<F> {
        WorkerFnWrapper(self)
    }
}

#[derive(Debug)]
pub struct AsyncWrapper<F: Future>(Shared<F>);

impl<O: Send + Sync + Clone + 'static, F: Future<Output = O> + Send + Sync> Node
    for AsyncWrapper<F>
{
    type Output = O;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        MaybeCached::Constant(w.block_on(self.0.clone()))
    }
}

impl<O: Send + Sync + Clone + 'static, F: Future<Output = O> + Send + Sync>
    IntoNode<AsyncWrapper<F>> for F
{
    fn into_node(self) -> AsyncWrapper<F> {
        use futures::future::FutureExt;
        AsyncWrapper(self.shared())
    }
}

pub struct CachedFnWrapper<O: Send, F: Fn(&Worker, InvalidatorGenerator<O>) -> O> {
    f: F,
    cache: Arc<Cache<O>>,
}

impl<O: Send, F: Fn(&Worker, InvalidatorGenerator<O>) -> O> CachedFnWrapper<O, F> {
    pub fn new(f: F) -> Self {
        CachedFnWrapper {
            f,
            cache: Cache::new_arc(),
        }
    }
}

impl<O: Send + Clone + 'static, F: Fn(&Worker, InvalidatorGenerator<O>) -> O + Send + Sync>
    IntoNode<CachedFnWrapper<O, F>> for F
{
    fn into_node(self) -> CachedFnWrapper<O, F> {
        CachedFnWrapper {
            f: self,
            cache: Cache::new_arc(),
        }
    }
}

impl<O: Send + Clone + 'static, F: Fn(&Worker, InvalidatorGenerator<O>) -> O + Send + Sync> Node
    for CachedFnWrapper<O, F>
{
    type Output = O;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        self.cache.resolve(w, |g| (self.f)(w, g), false)
    }
}

pub struct TupleWrapper<T, C>(T, Arc<Cache<C>>);

macro_rules! impl_into_node_for_tuple {
    ($($t:ident $t_i:ident),*) => {
        peregrine_macros::impl_op_for_tuple_wrapper!($($t),*);
        impl<$($t: Node, $t_i: IntoNode<$t>),*> IntoNode<TupleWrapper<($($t,)*), ($($t::Output,)*)>> for ($($t_i,)*) where $($t::Output: Clone + 'static),* {
            #[allow(non_snake_case)]
            fn into_node(self) -> TupleWrapper<($($t,)*), ($($t::Output,)*)> {
                let ($($t_i,)*) = self;
                TupleWrapper(($($t_i.into_node()),*), Cache::new_arc())
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

pub struct UnaryTupleWrapper<A: Node>(A);

impl<A: Node, AI: IntoNode<A>> IntoNode<UnaryTupleWrapper<A>> for (AI,) {
    fn into_node(self) -> UnaryTupleWrapper<A> {
        UnaryTupleWrapper(self.0.into_node())
    }
}

impl<A: Node> Node for UnaryTupleWrapper<A> {
    type Output = (A::Output,);

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        self.0.run(w).map(|v| (v,))
    }
}

impl<N: Node> Node for Option<N> {
    type Output = Option<N::Output>;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        match self {
            Some(node) => node.run(w).map(Some),
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

        assert_eq!(result, 5);
    }
}
