pub mod memo;
pub mod structure;
pub mod view;

use std::sync::Arc;

use async_trait::async_trait;
use smol::Executor;
use view::View;

#[async_trait]
pub trait Node: Send + Sync {
    type Output: Send;

    async fn run<'s>(&self, ex: Exec<'s>) -> Self::Output;

    fn should_not_spawn(&self) -> bool {
        false
    }
}

type DynNode<O> = Box<dyn Node<Output = O>>;

#[async_trait]
impl<N: Node> Node for &N {
    type Output = N::Output;

    async fn run<'s>(&self, ex: Exec<'s>) -> Self::Output {
        (**self).run(ex).await
    }

    fn should_not_spawn(&self) -> bool {
        (**self).should_not_spawn()
    }
}

#[async_trait]
impl<N: Node + ?Sized> Node for Box<N> {
    type Output = N::Output;

    async fn run<'s>(&self, ex: Exec<'s>) -> Self::Output {
        (**self).run(ex).await
    }

    fn should_not_spawn(&self) -> bool {
        (**self).should_not_spawn()
    }
}

#[async_trait]
impl<N: Node> Node for Arc<N> {
    type Output = N::Output;

    async fn run<'s>(&self, ex: Exec<'s>) -> Self::Output {
        (**self).run(ex).await
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

#[async_trait]
impl<O: View> Node for ViewWrapper<O> {
    type Output = O::Result;

    async fn run<'s>(&self, _: Exec<'s>) -> Self::Output {
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

#[async_trait]
impl<O: Send, F: Fn() -> O + Send + Sync> Node for FnNodeWrapper<F> {
    type Output = O;

    async fn run<'s>(&self, _ex: Exec<'s>) -> Self::Output {
        self.0()
    }
}

impl<O: Send, F: Fn() -> O + Send + Sync> IntoNode<FnNodeWrapper<F>> for F {
    fn into_node(self) -> FnNodeWrapper<F> {
        FnNodeWrapper(self)
    }
}

pub struct ExFnNodeWrapper<F>(F);

#[async_trait]
impl<O: Send, F: Fn(Exec<'_>) -> O + Send + Sync> Node for ExFnNodeWrapper<F> {
    type Output = O;

    async fn run<'s>(&self, ex: Exec<'s>) -> Self::Output {
        self.0(ex)
    }
}

impl<O: Send, F: Fn(Exec<'_>) -> O + Send + Sync> IntoNode<ExFnNodeWrapper<F>> for F {
    fn into_node(self) -> ExFnNodeWrapper<F> {
        ExFnNodeWrapper(self)
    }
}

#[derive(Clone)]
pub struct Exec<'s> {
    executor: Arc<Executor<'s>>,
    stack_counter: usize,
}

impl<'s> Exec<'s> {
    fn new() -> Self {
        Exec {
            executor: Arc::new(Executor::new()),
            stack_counter: 0,
        }
    }

    pub async fn run<O: Send + 's>(&self, node: &'s impl Node<Output = O>) -> O {
        self.executor.spawn(node.run(self.increment())).await
    }

    pub fn run_blocking<N: Node>(node: impl IntoNode<N>) -> N::Output {
        let ex = Exec::new();
        smol::block_on(ex.executor.run(node.into_node().run(ex.increment())))
    }

    fn increment(&self) -> Self {
        Exec {
            executor: self.executor.clone(),
            stack_counter: self.stack_counter + 1,
        }
    }
}

pub trait Init {
    type Value;

    fn init(value: Self::Value) -> Self;
}
