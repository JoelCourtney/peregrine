pub mod stages;
pub mod log;

use crate::{DynNode, Exec, IntoNode, Node};
use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};
use std::{collections::BTreeMap, ops::RangeBounds, sync::Arc};

struct OrderArc<I: Ord + Copy, V> {
    data: Arc<OrderData<I, V>>,
    index: MaybeInf<I>,
}

struct OrderData<I: Ord + Copy, V> {
    initial_condition: DynNode<V>,
    map: RwLock<BTreeMap<I, DynNode<V>>>,
    reference_indices: Mutex<BTreeMap<MaybeInf<I>, usize>>,
}

#[derive(Ord, PartialOrd, Eq, PartialEq, Copy, Clone, Debug)]
enum MaybeInf<I> {
    Value(I),
    Inf,
}

impl<I> MaybeInf<I> {
    fn unwrap_value(&self) -> &I {
        match self {
            MaybeInf::Value(value) => value,
            MaybeInf::Inf => panic!("Cannot unwrap value from MaybeInf::Inf"),
        }
    }
}

impl<I: Ord + Copy, V> OrderArc<I, V> {
    fn new(index: MaybeInf<I>, data: Arc<OrderData<I, V>>) -> Self {
        let mut indices_lock = data.reference_indices.lock();
        *indices_lock.entry(index).or_insert(0) += 1;
        drop(indices_lock);
        Self { data, index }
    }

    fn clone_at_index(&self, index: I) -> Self {
        Self::new(MaybeInf::Value(index), self.data.clone())
    }

    fn clone_at_end(&self) -> Self {
        Self::new(MaybeInf::Inf, self.data.clone())
    }
}

impl<I: Ord + Copy, V> Clone for OrderArc<I, V> {
    fn clone(&self) -> Self {
        Self::new(self.index, self.data.clone())
    }
}

impl<I: Ord + Copy, V> Drop for OrderArc<I, V> {
    fn drop(&mut self) {
        let mut indices = self
            .data
            .reference_indices
            .try_lock()
            .expect("Failed to acquire lock during drop");
        *indices.get_mut(&self.index).unwrap() -= 1;
        if indices.get(&self.index).unwrap() == &0 {
            indices.remove(&self.index);
        }
        let max = indices.last_key_value();
        if let Some((max_index, _)) = max {
            let max_index = *max_index;
            drop(indices);
            if max_index < self.index {
                let mut write_lock = self
                    .data
                    .map
                    .try_write()
                    .expect("Failed to acquire write lock during drop");
                let tail = write_lock.split_off(max_index.unwrap_value());
                drop(write_lock);
                drop(tail);
            }
        } else if indices.is_empty() {
            drop(indices);
            let mut write_lock = self
                .data
                .map
                .try_write()
                .expect("Failed to acquire write lock during drop");
            write_lock.clear();
        }
    }
}

pub struct Order<I: Ord + Copy, V>(OrderArc<I, V>);

impl<I: Ord + Copy, V> Order<I, V> {
    pub fn new<N: Node<Output = V> + 'static>(n: impl IntoNode<N>) -> Self {
        let inner = OrderArc::new(
            MaybeInf::Inf,
            Arc::new(OrderData {
                initial_condition: Box::new(n.into_node()),
                map: RwLock::new(BTreeMap::new()),
                reference_indices: Mutex::new(BTreeMap::new()),
            }),
        );
        Order(inner)
    }

    pub fn write<N: Node<Output = V> + 'static>(&mut self, index: I, n: impl IntoNode<N>) {
        self.0
            .data
            .map
            .try_write()
            .expect("Failed to acquire write lock")
            .insert(index, Box::new(n.into_node()));
    }

    pub fn read(&self, index: I) -> OrderUpstreamFinder<I, V> {
        OrderUpstreamFinder(self.0.clone_at_index(index))
    }

    pub fn read_at_end(&self) -> OrderUpstreamFinder<I, V> {
        OrderUpstreamFinder(self.0.clone_at_end())
    }
    
    pub fn collect(&self, index: I, filter: impl Fn(&I) -> bool + Send + Sync + 'static) -> OrderCollector<I, V> {
        OrderCollector(self.0.clone_at_index(index), Box::new(filter))
    }
    
    pub fn collect_at_end(&self, filter: impl Fn(&I) -> bool + Send + Sync + 'static) -> OrderCollector<I, V> {
        OrderCollector(self.0.clone_at_end(), Box::new(filter))
    }

    pub fn last_in_range(&self, range: impl RangeBounds<I>) -> Option<I> {
        self.0
            .data
            .map
            .try_read()
            .expect("Failed to acquire read lock")
            .range(range)
            .last()
            .map(|(k, _)| *k)
    }
}

#[derive(Clone)]
pub struct OrderUpstreamFinder<I: Ord + Copy, V>(OrderArc<I, V>);

#[async_trait]
impl<I: Copy + Ord + Send + Sync, V: Send + Sync> Node for OrderUpstreamFinder<I, V> {
    type Output = V;

    #[allow(clippy::await_holding_lock)]
    async fn run<'s>(&self, ex: Exec<'s>) -> Self::Output {
        let order = self
            .0
            .data
            .map
            .try_read()
            .expect("Failed to acquire read lock");
        let last = match &self.0.index {
            MaybeInf::Value(v) => order.range(..v).last(),
            MaybeInf::Inf => order.range(..).last(),
        };
        last.map(|(_, node)| node)
            .unwrap_or(&self.0.data.initial_condition)
            .run(ex)
            .await
    }
}

pub struct OrderCollector<I: Ord + Copy, V>(OrderArc<I, V>, Box<dyn Fn(&I) -> bool + Send + Sync>);

#[async_trait]
impl<I: Copy + Ord + Send + Sync, V: Send + Sync> Node for OrderCollector<I, V> {
    type Output = Vec<(I, V)>;

    #[allow(clippy::await_holding_lock)]
    async fn run<'s>(&self, ex: Exec<'s>) -> Self::Output {
        let order = self
            .0
            .data
            .map
            .try_read()
            .expect("Failed to acquire read lock");
        let iter = match &self.0.index {
            MaybeInf::Value(v) => order.range(..v),
            MaybeInf::Inf => order.range(..),
        };
        let mut result = vec![];
        for (i, n) in iter {
            if (self.1)(i) {
                result.push((*i, n.run(ex.increment()).await));
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::{ops::Add, sync::Arc};

    use async_trait::async_trait;

    use crate::{Exec, IntoNode, Node};

    use super::Order;

    struct AddNode<N: Node, M: Node>(N, M);

    #[async_trait]
    impl<N: Node, M: Node + std::fmt::Debug> Node for AddNode<N, M>
    where
        N::Output: Add<M::Output>,
        <N::Output as Add<M::Output>>::Output: Send + Sync,
    {
        type Output = <N::Output as Add<M::Output>>::Output;

        async fn run<'s>(&self, ex: Exec<'s>) -> Self::Output {
            self.0.run(ex.increment()).await + self.1.run(ex.increment()).await
        }
    }

    #[test]
    fn test_drop_mutual_recursion() {
        let mut order_a = Order::<&'static str, _>::new(0);
        let mut order_b = Order::<&'static str, _>::new(0);

        let node_1 = Arc::new(1.into_node());
        let node_2 = Arc::new(AddNode(order_a.read("aa"), 2.into_node()));
        let node_3 = Arc::new(AddNode(order_b.read_at_end(), 3.into_node()));
        order_a.write("a", node_1.clone());
        order_b.write("b", node_2.clone());
        order_a.write("c", node_3.clone());

        assert_eq!(Exec::run_blocking(&order_a.read_at_end()), 6);

        let weak_1 = Arc::downgrade(&node_1);
        let weak_2 = Arc::downgrade(&node_2);
        let weak_3 = Arc::downgrade(&node_3);

        drop(node_1);
        drop(node_2);
        drop(node_3);

        assert!(weak_1.upgrade().is_some());
        assert!(weak_2.upgrade().is_some());
        assert!(weak_3.upgrade().is_some());

        drop(order_b);

        assert!(weak_1.upgrade().is_some());
        assert!(weak_2.upgrade().is_some());
        assert!(weak_3.upgrade().is_some());

        drop(order_a);

        assert!(weak_1.upgrade().is_none());
        assert!(weak_2.upgrade().is_none());
        assert!(weak_3.upgrade().is_none());
    }

    #[test]
    fn test_drop_staggered() {
        let mut order_a = Order::<&'static str, _>::new(0);
        let mut order_b = Order::<&'static str, _>::new(0);

        let node_1 = Arc::new(1.into_node());
        let node_2 = Arc::new(AddNode(order_a.read("aa"), 2.into_node()));
        let node_3 = Arc::new(AddNode(order_b.read_at_end(), 3.into_node()));
        order_a.write("a", node_1.clone());
        order_b.write("b", node_2.clone());
        order_a.write("c", node_3.clone());

        assert_eq!(Exec::run_blocking(&order_a.read_at_end()), 6);

        let weak_1 = Arc::downgrade(&node_1);
        let weak_2 = Arc::downgrade(&node_2);
        let weak_3 = Arc::downgrade(&node_3);

        drop(node_1);
        drop(node_2);
        drop(node_3);

        assert!(weak_1.upgrade().is_some());
        assert!(weak_2.upgrade().is_some());
        assert!(weak_3.upgrade().is_some());

        drop(order_a);

        assert!(weak_1.upgrade().is_some());
        assert!(weak_2.upgrade().is_some());
        assert!(weak_3.upgrade().is_none());

        drop(order_b);

        assert!(weak_1.upgrade().is_none());
        assert!(weak_2.upgrade().is_none());
        assert!(weak_3.upgrade().is_none());
    }
}
