use std::sync::{Arc, OnceLock, Weak};

use async_lock::{Mutex, MutexGuard};
use derive_more::Deref;
use forte::Worker;

use crate::Node;

pub trait CacheableNode: Send + Sync {
    type Output: Send;

    fn run_with_receiver(&self, w: &Worker, r: &mut UpstreamReceiver<Self::Output>)
    -> Self::Output;
}

pub struct CachedNode<N: CacheableNode> {
    node: N,
    cache: Arc<Cache<N::Output>>,
}

impl<N: CacheableNode> CachedNode<N> {
    pub fn new(node: N) -> Self {
        Self {
            node,
            cache: Cache::new(),
        }
    }
}

impl<N: CacheableNode> Node for CachedNode<N>
where
    N::Output: Clone + 'static,
{
    type Output = N::Output;

    fn run_cache(&self, w: &Worker) -> MaybeCached<Self::Output> {
        self.cache
            .try_resolve(|r| self.node.run_with_receiver(w, r))
            .unwrap_or_else(|| {
                w.block_on(self.cache.resolve(|r| {
                    Worker::with_current(|w| {
                        self.node
                            .run_with_receiver(w.expect("Expected to be run on thread pool"), r)
                    })
                }))
            })
    }
}

pub enum MaybeCached<T> {
    Cached(Upstream<T>),
    Constant(T),
    Uncached(T),
}

impl<T> MaybeCached<T> {
    pub fn open(self) -> T {
        match self {
            MaybeCached::Cached(upstream) => upstream.value,
            MaybeCached::Constant(value) => value,
            MaybeCached::Uncached(value) => value,
        }
    }
}

#[derive(Deref)]
pub struct Cache<T>(Mutex<Option<InnerCache<T>>>);

impl<T> Cache<T> {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(None)))
    }

    pub fn clear(&self) {
        *self.0.lock_blocking() = None;
    }

    pub fn is_valid(&self) -> bool {
        self.0.lock_blocking().is_some()
    }

    pub async fn resolve(
        self: &Arc<Self>,
        f: impl FnOnce(&mut UpstreamReceiver<T>) -> T,
    ) -> MaybeCached<T>
    where
        T: Clone + 'static,
    {
        self.resolve_internal(self.0.lock().await, f)
    }

    pub fn try_resolve(
        self: &Arc<Self>,
        f: impl FnOnce(&mut UpstreamReceiver<T>) -> T,
    ) -> Option<MaybeCached<T>>
    where
        T: Clone + 'static,
    {
        Some(self.resolve_internal(self.0.try_lock()?, f))
    }

    pub fn resolve_blocking(
        self: &Arc<Self>,
        f: impl FnOnce(&mut UpstreamReceiver<T>) -> T,
    ) -> MaybeCached<T>
    where
        T: Clone + 'static,
    {
        self.resolve_internal(self.0.lock_blocking(), f)
    }

    fn resolve_internal(
        self: &Arc<Self>,
        mut guard: MutexGuard<Option<InnerCache<T>>>,
        f: impl FnOnce(&mut UpstreamReceiver<T>) -> T,
    ) -> MaybeCached<T>
    where
        T: Clone + 'static,
    {
        match &mut *guard {
            Some(inner) => MaybeCached::Cached(inner.upstream()),
            i @ None => {
                let downstream = Downstream(Arc::downgrade(self));
                let mut receiver = UpstreamReceiver {
                    cacheable: true,
                    downstream,
                };
                let value = f(&mut receiver);
                if receiver.cacheable {
                    let mut new_cache = InnerCache::new(value.clone());
                    let upstream = new_cache.upstream();
                    *i = Some(new_cache);
                    drop(guard);
                    MaybeCached::Cached(upstream)
                } else {
                    MaybeCached::Uncached(value)
                }
            }
        }
    }
}

pub struct InnerCache<T> {
    value: T,
    downstreams: Vec<Arc<OnceLock<Box<dyn ErasedDownstream>>>>,
}

impl<T: Clone + 'static> InnerCache<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            downstreams: Vec::new(),
        }
    }

    pub fn upstream(&mut self) -> Upstream<T> {
        let cell = Arc::new(OnceLock::new());
        self.downstreams.push(cell.clone());
        Upstream {
            value: self.value.clone(),
            callback: cell,
        }
    }
}

impl<T> Drop for InnerCache<T> {
    fn drop(&mut self) {
        for downstream in self.downstreams.drain(..) {
            if let Some(downstream) = downstream.get() {
                downstream.invalidate();
            }
        }
    }
}

pub struct Upstream<T> {
    value: T,
    callback: Arc<OnceLock<Box<dyn ErasedDownstream>>>,
}

pub struct Downstream<T>(Weak<Cache<T>>);

trait ErasedDownstream: Send + Sync {
    fn invalidate(&self);
}

impl<T> Downstream<T> {
    pub fn empty() -> Self {
        Downstream(Weak::new())
    }
}

impl<T: Send> ErasedDownstream for Downstream<T> {
    fn invalidate(&self) {
        if let Some(cache) = self.0.upgrade() {
            cache.clear();
        }
    }
}

impl<T> Clone for Downstream<T> {
    fn clone(&self) -> Self {
        Downstream(self.0.clone())
    }
}

pub struct UpstreamReceiver<T> {
    cacheable: bool,
    downstream: Downstream<T>,
}

impl<T: Send + 'static> UpstreamReceiver<T> {
    pub fn track<U>(&mut self, maybe_cached: MaybeCached<U>) -> U {
        match maybe_cached {
            MaybeCached::Cached(u) => {
                u.callback
                    .set(Box::new(self.downstream.clone()))
                    .ok()
                    .expect("Upstream callback already set");
                u.value
            }
            MaybeCached::Constant(value) => value,
            MaybeCached::Uncached(u) => {
                self.cacheable = false;
                u
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use forte::ThreadPool;

    use super::*;

    static COMPUTE: ThreadPool = ThreadPool::new();

    #[test]
    fn test_cache_manual_invalidation() {
        COMPUTE.resize_to_available();

        let cache_a = Cache::<usize>::new();
        let cache_b = Cache::<String>::new();

        let a = COMPUTE.block_on(cache_a.resolve(|_| 42));

        assert!(matches!(a, MaybeCached::Cached(_)));
        assert!(cache_a.is_valid());

        let b = COMPUTE.block_on(cache_b.resolve(|r| {
            let a = r.track(a);
            a.to_string()
        }));

        assert!(matches!(b, MaybeCached::Cached(_)));
        assert_eq!(b.open(), "42");

        assert!(cache_b.is_valid());

        cache_a.clear();
        assert!(!cache_b.is_valid());
    }

    #[test]
    fn test_cache_auto_invalidation() {
        COMPUTE.resize_to_available();

        let cache_a = Cache::<usize>::new();
        let cache_b = Cache::<String>::new();

        let a = COMPUTE.block_on(cache_a.resolve(|_| 42));

        assert!(matches!(a, MaybeCached::Cached(_)));
        assert!(cache_a.is_valid());

        let b = COMPUTE.block_on(cache_b.resolve(|r| {
            let a = r.track(a);
            a.to_string()
        }));

        assert!(matches!(b, MaybeCached::Cached(_)));
        assert_eq!(b.open(), "42");

        assert!(cache_b.is_valid());

        drop(cache_a);
        assert!(!cache_b.is_valid());
    }
}
