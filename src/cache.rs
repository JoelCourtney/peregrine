use std::sync::{Arc, atomic::AtomicBool};

use async_lock::{Mutex, MutexGuard};
use forte::Worker;
use oneshot::{Receiver, Sender, channel};

use crate::{DynNode, IntoNode, Node};

#[derive(Default)]
enum DataState<T> {
    Constant(T),
    Valid(T),
    Invalid(T),
    #[default]
    Empty,
}

impl<T> DataState<T> {
    fn invalidate(&mut self) {
        match std::mem::take(self) {
            DataState::Valid(d) | DataState::Invalid(d) => *self = DataState::Invalid(d),
            DataState::Constant(_) => unreachable!(),
            _ => {}
        }
    }
}

type Invalidator = Box<dyn FnOnce() + Send>;
pub struct Cache<T> {
    data: Mutex<DataState<T>>,
    downstreams: parking_lot::Mutex<Vec<Receiver<Invalidator>>>,
}

impl<T> Cache<T> {
    pub fn new() -> Arc<Cache<T>> {
        Arc::new(Cache {
            data: Mutex::new(DataState::Empty),
            downstreams: parking_lot::Mutex::new(vec![]),
        })
    }
    fn resolve_internal(
        self: &Arc<Self>,
        f: impl FnOnce(InvalidatorGenerator<T>) -> T,
        mut lock: MutexGuard<DataState<T>>,
        force_variable: bool,
    ) -> MaybeCached<T>
    where
        T: Send + Clone,
    {
        let (data, constant) = match &mut *lock {
            DataState::Constant(d) => (d.clone(), true),
            DataState::Valid(d) => (d.clone(), false),
            DataState::Invalid(_d) => todo!(),
            state @ DataState::Empty => {
                let cell = AtomicBool::new(false);
                let result = f(InvalidatorGenerator(self, &cell));
                let generator_used = cell.load(std::sync::atomic::Ordering::Relaxed);
                *state = if generator_used || force_variable {
                    DataState::Valid(result.clone())
                } else {
                    DataState::Constant(result.clone())
                };
                (result, !generator_used)
            }
        };
        if !constant || force_variable {
            let (send, recv) = channel();
            self.downstreams.lock().push(recv);
            MaybeCached::Cached(data, send)
        } else {
            MaybeCached::Constant(data)
        }
    }
    pub fn resolve(
        self: &Arc<Self>,
        w: &Worker,
        f: impl FnOnce(InvalidatorGenerator<T>) -> T,
        force_variable: bool,
    ) -> MaybeCached<T>
    where
        T: Send + Clone,
    {
        let lock = self
            .data
            .try_lock()
            .unwrap_or_else(|| w.block_on(self.data.lock()));
        self.resolve_internal(f, lock, force_variable)
    }
    pub fn resolve_blocking(
        self: &Arc<Self>,
        f: impl FnOnce(InvalidatorGenerator<T>) -> T,
        force_variable: bool,
    ) -> MaybeCached<T>
    where
        T: Send + Clone,
    {
        self.resolve_internal(f, self.data.lock_blocking(), force_variable)
    }
    pub fn invalidate(&self) {
        self.data.lock_blocking().invalidate();
        for downstream in self.downstreams.lock().drain(..) {
            if let Ok(inv) = downstream.recv() {
                inv();
            }
        }
    }
    pub fn is_valid(&self) -> bool {
        matches!(
            &*self.data.lock_blocking(),
            DataState::Valid(_) | DataState::Constant(_)
        )
    }
}

pub struct InvalidatorGenerator<'a, T>(&'a Arc<Cache<T>>, &'a AtomicBool);

impl<T> Clone for InvalidatorGenerator<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for InvalidatorGenerator<'_, T> {}

impl<T: 'static> InvalidatorGenerator<'_, T> {
    fn get(&self) -> impl FnOnce() + 'static {
        self.1.store(true, std::sync::atomic::Ordering::Relaxed);
        let weak = Arc::downgrade(self.0);
        move || {
            if let Some(c) = weak.upgrade() {
                c.invalidate()
            }
        }
    }
}

pub enum MaybeCached<T> {
    Cached(T, Sender<Invalidator>),
    Constant(T),
    Uncached(T),
}

impl<T> MaybeCached<T> {
    pub fn track<G: Send + 'static>(self, generator: InvalidatorGenerator<G>) -> T
    where
        T: Send + 'static,
    {
        match self {
            MaybeCached::Cached(value, sender) => {
                sender
                    .send(Box::new(generator.get()))
                    .expect("Could not send invalidator");
                value
            }
            MaybeCached::Constant(value) => value,
            MaybeCached::Uncached(value) => value,
        }
    }

    pub fn open(self) -> T
    where
        T: Send + 'static,
    {
        match self {
            MaybeCached::Cached(value, _) => value,
            MaybeCached::Constant(value) => value,
            MaybeCached::Uncached(value) => value,
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> MaybeCached<U> {
        match self {
            MaybeCached::Cached(value, sender) => MaybeCached::Cached(f(value), sender),
            MaybeCached::Constant(value) => MaybeCached::Constant(f(value)),
            MaybeCached::Uncached(value) => MaybeCached::Uncached(f(value)),
        }
    }
}

pub struct NodeCell<O>(Mutex<(DynNode<O>, Arc<Cache<O>>)>);

impl<O> NodeCell<O> {
    pub fn new<N: Node<Output = O> + 'static>(node: impl IntoNode<N>) -> Self {
        NodeCell(Mutex::new((Box::new(node.into_node()), Cache::new())))
    }

    pub fn set<N: Node<Output = O> + 'static>(&self, node: impl IntoNode<N>) {
        let mut lock = self.0.lock_blocking();
        lock.0 = Box::new(node.into_node());
        lock.1.invalidate()
    }
}

impl<O: Clone + Send + 'static> Node for NodeCell<O> {
    type Output = O;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        let lock = self.0.lock_blocking();
        lock.1.resolve(w, |g| lock.0.run(w).track(g), true)
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

        let b = cache_b.resolve_blocking(|i| a.track(i).to_string(), false);

        assert!(matches!(b, MaybeCached::Cached(_, _)));
        assert_eq!(b.open(), "42");

        assert!(cache_b.is_valid());

        node_a.set(5);
        assert!(!cache_b.is_valid());
    }
}
