use std::sync::Weak;

use async_trait::async_trait;
use derive_more::Deref;
use smol::lock::Mutex;

use crate::{Exec, Node, read::Readable};

#[derive(Deref)]
pub struct Cached<N: Node> {
    #[deref]
    node: N,
    result: Mutex<Option<N::Output>>,
    dependents: Vec<Weak<dyn ErasedCached>>,
}

impl<N: Node> Cached<N> {
    pub fn new(node: N) -> Self {
        Self {
            node,
            result: Mutex::new(None),
            dependents: Vec::new(),
        }
    }
    
    pub fn clear_cache(&self) {
        *(self.result.lock_blocking()) = None;
        for dependent in &self.dependents {
            if let Some(dependent) = dependent.upgrade() {
                dependent.clear_cache();
            }
        }
    }
}

pub trait ErasedCached: Send + Sync {
    fn clear_cache(&self);
}

impl<N: Node> ErasedCached for Cached<N> {
    fn clear_cache(&self) {
        self.clear_cache();
    }
}

#[async_trait]
impl<N: Node> Node for Cached<N>
where
    N::Output: Readable,
{
    type Output = <N::Output as Readable>::Read;

    async fn run<'s>(&self, ex: Exec<'s>) -> Self::Output {
        let mut result = self.result.lock().await;
        match result.as_ref() {
            Some(output) => output.read(),
            None => {
                let output = self.node.run(ex).await;
                let read = output.read();
                *result = Some(output);
                read
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Exec, Node, cache::Cached};
    use async_trait::async_trait;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn cache() {
        struct A(AtomicU32);

        #[async_trait]
        impl Node for A {
            type Output = usize;

            async fn run<'s>(&self, _env: Exec<'s>) -> Self::Output {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                6
            }
        }

        let a1 = Cached::new(A(AtomicU32::new(0)));
        let a2 = Cached::new(A(AtomicU32::new(0)));

        assert_eq!(a1.0.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(a2.0.load(std::sync::atomic::Ordering::SeqCst), 0);

        assert_eq!(Exec::run_blocking(&a1), 6);
        assert_eq!(Exec::run_blocking(&a2), 6);

        assert_eq!(a1.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(a2.0.load(std::sync::atomic::Ordering::SeqCst), 1);

        assert_eq!(Exec::run_blocking(&a1), 6);
        assert_eq!(Exec::run_blocking(&a2), 6);

        assert_eq!(a1.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(a2.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
