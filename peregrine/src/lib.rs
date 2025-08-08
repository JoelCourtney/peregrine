pub mod cache;
pub mod memo;
pub mod read;

use std::sync::Arc;

use async_trait::async_trait;
use smol::Executor;

#[async_trait]
pub trait Node: Send + Sync {
    type Context: Send + Sync;
    type Output: Send;

    async fn run<'s>(&self, ctx: &'s Self::Context, ex: Exec<'s>) -> Self::Output;
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

    pub async fn run<C, O: Send + 's>(
        &self,
        ctx: &'s C,
        node: &'s impl Node<Context = C, Output = O>,
    ) -> O {
        self.executor.spawn(node.run(ctx, self.increment())).await
    }

    pub fn run_blocking<C, O: Send + 's>(ctx: &C, node: &impl Node<Context = C, Output = O>) -> O {
        let ex = Exec::new();
        smol::block_on(ex.executor.run(node.run(ctx, ex.increment())))
    }

    fn increment(&self) -> Self {
        Exec {
            executor: self.executor.clone(),
            stack_counter: self.stack_counter + 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use smol::lock::Mutex;

    use super::*;

    #[test]
    fn test() {
        struct A;

        #[async_trait]
        impl Node for A {
            type Context = ();
            type Output = usize;

            async fn run<'s>(&self, _ctx: &'s (), _env: Exec<'s>) -> usize {
                5
            }
        }

        struct B(Mutex<Option<Weak<A>>>);

        #[async_trait]
        impl Node for B {
            type Context = Arc<A>;
            type Output = String;

            async fn run<'s>(&self, ctx: &'s Arc<A>, ex: Exec<'s>) -> String {
                *(self.0.lock().await) = Some(Arc::downgrade(ctx));
                format!("Hello, {}", ex.run(&(), &**ctx).await)
            }
        }

        let result = Exec::run_blocking(&Arc::new(A), &B(Mutex::new(None)));
        assert_eq!(result, "Hello, 5");
    }

    #[test]
    fn test_vec() {
        struct A {
            index: usize,
            value: u32,
        }

        #[async_trait]
        impl Node for A {
            type Context = Vec<A>;
            type Output = u32;

            async fn run<'s>(&self, ctx: &'s Vec<A>, ex: Exec<'s>) -> u32 {
                if self.index == 0 {
                    self.value
                } else {
                    let upstream = &ctx[self.index - 1];
                    self.value + ex.run(ctx, upstream).await
                }
            }
        }

        let v = vec![
            A { index: 0, value: 1 },
            A { index: 1, value: 2 },
            A { index: 2, value: 3 },
        ];

        let result = Exec::run_blocking(&v, &v[2]);
        assert_eq!(result, 6);
    }
}
