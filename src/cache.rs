use std::sync::{Arc, OnceLock, Weak};

use async_lock::{Mutex, MutexGuard};
use derive_more::Deref;
use forte::Worker;
use parking_lot::RwLock;

use crate::{DynNode, IntoNode, Node};

pub fn merge_tuple<A, B>((a, b): (MaybeCached<A>, MaybeCached<B>)) -> MaybeCached<(A, B)> {
    a.merge(b, |a, b| (a, b))
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

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> MaybeCached<U> {
        match self {
            MaybeCached::Cached(up) => MaybeCached::Cached(Upstream {
                value: f(up.value),
                callbacks: up.callbacks,
            }),
            MaybeCached::Constant(value) => MaybeCached::Constant(f(value)),
            MaybeCached::Uncached(value) => MaybeCached::Uncached(f(value)),
        }
    }

    pub fn merge<U, O>(self, other: MaybeCached<U>, f: impl FnOnce(T, U) -> O) -> MaybeCached<O> {
        use MaybeCached::*;
        match (self, other) {
            (s @ Uncached(_), o) | (s, o @ Uncached(_)) => Uncached(f(s.open(), o.open())),
            (Constant(s), Constant(o)) => Constant(f(s, o)),
            (Cached(s), Constant(o)) => Cached(Upstream {
                value: f(s.value, o),
                callbacks: s.callbacks,
            }),
            (Constant(s), Cached(o)) => Cached(Upstream {
                value: f(s, o.value),
                callbacks: o.callbacks,
            }),
            (Cached(mut t), Cached(mut u)) => Cached(Upstream {
                value: f(t.value, u.value),
                callbacks: {
                    t.callbacks.append(&mut u.callbacks);
                    t.callbacks
                },
            }),
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
            Some(inner) => inner.get(),
            i @ None => {
                let downstream = Downstream(Arc::downgrade(self));
                let mut receiver = UpstreamReceiver {
                    constant: true,
                    cacheable: true,
                    downstream,
                };
                let value = f(&mut receiver);
                if receiver.cacheable {
                    let (new_cache, result) = InnerCache::new(value, receiver.constant);
                    *i = Some(new_cache);
                    drop(guard);
                    result
                } else {
                    MaybeCached::Uncached(value)
                }
            }
        }
    }
}

pub enum InnerCache<T> {
    Constant(T),
    Cached {
        value: T,
        downstreams: Vec<Arc<OnceLock<Box<dyn ErasedDownstream>>>>,
    },
}

impl<T: Clone + 'static> InnerCache<T> {
    pub fn new(value: T, constant: bool) -> (Self, MaybeCached<T>) {
        if constant {
            (Self::Constant(value.clone()), MaybeCached::Constant(value))
        } else {
            let v = vec![Arc::new(OnceLock::new())];
            (
                Self::Cached {
                    value: value.clone(),
                    downstreams: v.clone(),
                },
                MaybeCached::Cached(Upstream {
                    value,
                    callbacks: v,
                }),
            )
        }
    }

    pub fn get(&mut self) -> MaybeCached<T> {
        match self {
            Self::Constant(value) => MaybeCached::Constant(value.clone()),
            Self::Cached { value, downstreams } => {
                let downstream = Arc::new(OnceLock::new());
                downstreams.push(downstream.clone());
                MaybeCached::Cached(Upstream {
                    value: value.clone(),
                    callbacks: vec![downstream],
                })
            }
        }
    }
}

impl<T> Drop for InnerCache<T> {
    fn drop(&mut self) {
        match self {
            Self::Constant(_) => {}
            Self::Cached { downstreams, .. } => {
                for downstream in downstreams.drain(..) {
                    if let Some(downstream) = downstream.get() {
                        downstream.invalidate();
                    }
                }
            }
        }
    }
}

pub struct Upstream<T> {
    value: T,
    callbacks: Vec<Arc<OnceLock<Box<dyn ErasedDownstream>>>>,
}

pub struct Downstream<T>(Weak<Cache<T>>);

pub trait ErasedDownstream: Send + Sync {
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
    constant: bool,
    downstream: Downstream<T>,
}

impl<T: Send + 'static> UpstreamReceiver<T> {
    pub fn track<U>(&mut self, maybe_cached: MaybeCached<U>) -> U {
        match maybe_cached {
            MaybeCached::Cached(u) => {
                self.constant = false;
                for c in u.callbacks {
                    c.set(Box::new(self.downstream.clone()))
                        .ok()
                        .expect("Upstream callback already set");
                }
                u.value
            }
            MaybeCached::Constant(value) => value,
            MaybeCached::Uncached(u) => {
                self.cacheable = false;
                self.constant = false;
                u
            }
        }
    }

    pub fn force_variable(&mut self) {
        self.constant = false;
    }

    pub fn force_uncached(&mut self) {
        self.cacheable = false;
        self.constant = false;
    }
}

pub struct NodeCell<O>(RwLock<DynNode<O>>, Arc<Cache<()>>);

impl<O> NodeCell<O> {
    pub fn new<N: Node<Output = O> + 'static>(node: impl IntoNode<N>) -> Self {
        NodeCell(RwLock::new(Box::new(node.into_node())), Cache::new())
    }

    pub fn set<N: Node<Output = O> + 'static>(&self, node: impl IntoNode<N>) {
        *self.0.write() = Box::new(node.into_node());
        self.1.clear();
    }
}

impl<O: Send> Node for NodeCell<O> {
    type Output = O;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        self.1
            .resolve_blocking(|r| {
                r.force_variable();
            })
            .merge(self.0.read().run(w), |_, v| v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Node;
    use forte::{ThreadPool, Worker};

    static COMPUTE: ThreadPool = ThreadPool::new();

    #[test]
    fn test_cache_manual_invalidation() {
        COMPUTE.resize_to_available();

        let node_a = NodeCell::new(42);
        let cache_b = Cache::<String>::new();

        let a = COMPUTE.block_on(async { Worker::with_current(|w| node_a.run(w.unwrap())) });

        let b = cache_b.resolve_blocking(|r| {
            let a = r.track(a);
            a.to_string()
        });

        assert!(matches!(b, MaybeCached::Cached(_)));
        assert_eq!(b.open(), "42");

        assert!(cache_b.is_valid());

        node_a.set(5);
        assert!(!cache_b.is_valid());
    }

    #[test]
    fn test_cache_auto_invalidation() {
        COMPUTE.resize_to_available();

        let node_a = NodeCell::new(42);
        let cache_b = Cache::<String>::new();

        let a = COMPUTE.block_on(async { Worker::with_current(|w| node_a.run(w.unwrap())) });

        let b = cache_b.resolve_blocking(|r| {
            let a = r.track(a);
            a.to_string()
        });

        assert!(matches!(b, MaybeCached::Cached(_)));
        assert_eq!(b.open(), "42");

        assert!(cache_b.is_valid());

        drop(node_a);
        assert!(!cache_b.is_valid());
    }

    #[test]
    fn test_cache_constant_not_invalidated() {
        let cache_a = Cache::<usize>::new();
        let cache_b = Cache::<String>::new();

        let a = cache_a.resolve_blocking(|_| 42);

        assert!(matches!(a, MaybeCached::Constant(_)));
        assert!(cache_a.is_valid());

        let b = cache_b.resolve_blocking(|r| {
            let a = r.track(a);
            a.to_string()
        });

        assert!(matches!(b, MaybeCached::Constant(_)));
        assert_eq!(b.open(), "42");

        assert!(cache_b.is_valid());

        drop(cache_a);
        assert!(cache_b.is_valid());
    }
}
