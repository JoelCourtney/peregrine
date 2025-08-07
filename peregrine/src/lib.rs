use std::sync::Arc;

use async_trait::async_trait;
use smol::Executor;

#[async_trait]
pub trait Node {
    type System;
    type Output;

    async fn run<'s>(&self, sys: &'s Self::System, env: Exec<'s>) -> Self::Output;
}

#[derive(Clone)]
pub struct Exec<'s> {
    executor: Arc<Executor<'s>>,
    stack_counter: usize,
    _history: (),
}

impl<'s> Exec<'s> {
    fn new() -> Self {
        Exec {
            executor: Arc::new(Executor::new()),
            stack_counter: 0,
            _history: (),
        }
    }

    pub async fn run<S, O: Send + 's>(
        &self,
        sys: &'s S,
        node: &'s impl Node<System = S, Output = O>,
    ) -> O {
        self.executor.spawn(node.run(sys, self.increment())).await
    }

    pub fn run_blocking<S, O: Send + 's>(sys: &S, node: &impl Node<System = S, Output = O>) -> O {
        let exec = Exec::new();
        smol::block_on(exec.executor.run(node.run(sys, exec.increment())))
    }

    fn increment(&self) -> Self {
        Exec {
            executor: self.executor.clone(),
            stack_counter: self.stack_counter + 1,
            _history: (),
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
            type System = ();
            type Output = usize;

            async fn run<'s>(&self, _sys: &'s (), _env: Exec<'s>) -> usize {
                5
            }
        }

        struct B(Mutex<Option<Weak<A>>>);

        #[async_trait]
        impl Node for B {
            type System = Arc<A>;
            type Output = String;

            async fn run<'s>(&self, sys: &'s Arc<A>, env: Exec<'s>) -> String {
                *(self.0.lock().await) = Some(Arc::downgrade(sys));
                format!("Hello, {}", env.run(&(), &**sys).await)
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
            type System = Vec<A>;
            type Output = u32;

            async fn run<'s>(&self, sys: &'s Vec<A>, env: Exec<'s>) -> u32 {
                if self.index == 0 {
                    self.value
                } else {
                    let upstream = &sys[self.index - 1];
                    self.value + env.run(sys, upstream).await
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
