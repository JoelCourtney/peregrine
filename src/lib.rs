pub mod memo;
pub mod structure;
pub mod view;

use std::sync::Arc;

use forte::{ThreadPool, Worker};
use view::View;

pub trait Node: Send + Sync {
    type Output: Send;

    fn run(&self, s: &Worker) -> Self::Output;

    fn should_not_spawn(&self) -> bool {
        false
    }
}

type DynNode<O> = Box<dyn Node<Output = O>>;

impl<N: Node> Node for &N {
    type Output = N::Output;

    fn run(&self, s: &Worker) -> Self::Output {
        (**self).run(s)
    }

    fn should_not_spawn(&self) -> bool {
        (**self).should_not_spawn()
    }
}

impl<N: Node + ?Sized> Node for Box<N> {
    type Output = N::Output;

    fn run(&self, s: &Worker) -> Self::Output {
        (**self).run(s)
    }

    fn should_not_spawn(&self) -> bool {
        (**self).should_not_spawn()
    }
}

impl<N: Node> Node for Arc<N> {
    type Output = N::Output;

    fn run(&self, s: &Worker) -> Self::Output {
        (**self).run(s)
    }

    fn should_not_spawn(&self) -> bool {
        (**self).should_not_spawn()
    }
}

pub trait IntoNode<N: Node> {
    fn into_node(self) -> N;
}

#[derive(Debug)]
pub struct ViewWrapper<O>(O);

impl<O: View> Node for ViewWrapper<O> {
    type Output = O::Result;

    fn run(&self, _: &Worker) -> Self::Output {
        self.0.view()
    }

    fn should_not_spawn(&self) -> bool {
        true
    }
}

impl<O: View> IntoNode<ViewWrapper<O>> for O {
    fn into_node(self) -> ViewWrapper<O> {
        ViewWrapper(self)
    }
}

impl<N: Node> IntoNode<N> for N {
    fn into_node(self) -> N {
        self
    }
}

pub struct FnNodeWrapper<F>(F);

impl<O: Send, F: Fn() -> O + Send + Sync> Node for FnNodeWrapper<F> {
    type Output = O;

    fn run(&self, _: &Worker) -> Self::Output {
        self.0()
    }
}

impl<O: Send, F: Fn() -> O + Send + Sync> IntoNode<FnNodeWrapper<F>> for F {
    fn into_node(self) -> FnNodeWrapper<F> {
        FnNodeWrapper(self)
    }
}

pub struct ExFnNodeWrapper<F>(F);

impl<O: Send, F: Fn(&Worker) -> O + Send + Sync> Node for ExFnNodeWrapper<F> {
    type Output = O;

    fn run(&self, s: &Worker) -> Self::Output {
        self.0(s)
    }
}

impl<O: Send, F: Fn(&Worker) -> O + Send + Sync> IntoNode<ExFnNodeWrapper<F>> for F {
    fn into_node(self) -> ExFnNodeWrapper<F> {
        ExFnNodeWrapper(self)
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
