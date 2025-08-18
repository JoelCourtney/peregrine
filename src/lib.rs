pub mod cache;
pub mod data;
pub mod macro_prelude;
pub mod memo;
pub mod structure;

pub use peregrine_macros::node;

use std::sync::Arc;

use cache::MaybeCached;
use data::Data;
use forte::{ThreadPool, Worker};

pub trait Node: Send + Sync {
    type Output: Send;

    fn run(&self, s: &Worker) -> Self::Output {
        self.run_cache(s).open()
    }

    fn run_cache(&self, s: &Worker) -> MaybeCached<Self::Output> {
        MaybeCached::Uncached(self.run(s))
    }
}

type DynNode<O> = Box<dyn Node<Output = O>>;

impl<N: Node> Node for &N {
    type Output = N::Output;

    fn run(&self, w: &Worker) -> Self::Output {
        (**self).run(w)
    }

    fn run_cache(&self, w: &Worker) -> MaybeCached<Self::Output> {
        (**self).run_cache(w)
    }
}

impl<N: Node + ?Sized> Node for Box<N> {
    type Output = N::Output;

    fn run(&self, w: &Worker) -> Self::Output {
        (**self).run(w)
    }

    fn run_cache(&self, w: &Worker) -> MaybeCached<Self::Output> {
        (**self).run_cache(w)
    }
}

impl<N: Node> Node for Arc<N> {
    type Output = N::Output;

    fn run(&self, w: &Worker) -> Self::Output {
        (**self).run(w)
    }

    fn run_cache(&self, w: &Worker) -> MaybeCached<Self::Output> {
        (**self).run_cache(w)
    }
}

pub trait IntoNode<N: Node> {
    fn into_node(self) -> N;
}

#[derive(Debug)]
pub struct DataWrapper<O>(O);

impl<O: Data> Node for DataWrapper<O> {
    type Output = O;

    fn run_cache(&self, _: &Worker) -> MaybeCached<Self::Output> {
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

impl<O: Send, F: Fn(&Worker) -> O + Send + Sync> Node for FnWrapper<F> {
    type Output = O;

    fn run(&self, w: &Worker) -> Self::Output {
        self.0(w)
    }
}

impl<O: Send, F: Fn(&Worker) -> O + Send + Sync> IntoNode<FnWrapper<F>> for F {
    fn into_node(self) -> FnWrapper<F> {
        FnWrapper(self)
    }
}

impl<N: Node> IntoNode<N> for N {
    fn into_node(self) -> N {
        self
    }
}

pub trait Init {
    type Value;

    fn init(value: Self::Value) -> Self;
}

pub fn run<N: Node>(node: impl IntoNode<N>) -> N::Output {
    static COMPUTE: ThreadPool = ThreadPool::new();
    COMPUTE.resize_to_available();

    COMPUTE.with_worker(|w| node.into_node().run(w))
}
