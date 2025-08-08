pub mod cache;
pub mod memo;
pub mod read;

use std::sync::Arc;

use async_trait::async_trait;
use smol::Executor;

#[async_trait]
pub trait Node: Send + Sync {
    type Output: Send;

    async fn run<'s>(&self, ex: Exec<'s>) -> Self::Output;
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

    pub fn run_blocking<O: Send + 's>(node: &impl Node<Output = O>) -> O {
        let ex = Exec::new();
        smol::block_on(ex.executor.run(node.run(ex.increment())))
    }

    fn increment(&self) -> Self {
        Exec {
            executor: self.executor.clone(),
            stack_counter: self.stack_counter + 1,
        }
    }
}
