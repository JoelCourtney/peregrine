use std::{
    fmt::Debug,
    ops::{Add, Mul},
};

use crate::{Init, IntoNode, Node, data::Data, node};

use super::{Order, OrderUpstreamFinder};

pub struct Stages<S: Ord + Copy, V>(Order<(S, usize), V>);

impl<S: Ord + Copy, V: Data> Init for Stages<S, V> {
    type Value = V;

    fn init(value: Self::Value) -> Self {
        Self::new(value)
    }
}

impl<S: Ord + Copy, V> Stages<S, V> {
    pub fn new<N: Node<Output = V> + 'static>(value: impl IntoNode<N>) -> Self {
        Self(Order::new(value))
    }
}

impl<S: Ord + Copy + Debug, V> Stages<S, V> {
    pub fn update<N: Node<Output = V> + 'static, IN: IntoNode<N>>(
        &mut self,
        stage: S,
        f: impl FnOnce(OrderUpstreamFinder<(S, usize), V>) -> IN,
    ) {
        let index = if let Some(i) = self
            .0
            .last_in_range((stage, 0)..(stage, usize::MAX))
            .map(|(_, index)| index)
        {
            if i == usize::MAX {
                panic!(
                    "Cannot update at stage {stage:?} because it is already being set; if a stage contains a set, it can be the only node in the stage."
                );
            }
            i + 1
        } else {
            0
        };
        self.0
            .write((stage, index), f(self.0.read((stage, index))).into_node())
    }

    pub fn set<N: Node<Output = V> + 'static>(&mut self, stage: S, node: impl IntoNode<N>) {
        let index = self
            .0
            .last_in_range((stage, 0)..(stage, usize::MAX))
            .map(|(_, index)| index);
        if index.is_some() {
            panic!(
                "Cannot set at stage {stage:?} because it is already updated or set; if a stage contains a set, it can be the only node in the stage."
            );
        }
        self.0.write((stage, usize::MAX), node.into_node())
    }
}

impl<S: 'static + Send + Sync + Ord + Copy + Debug, V> Stages<S, V>
where
    V: Send + Sync + 'static,
{
    pub fn add<N: Node<Output = V> + 'static>(&mut self, stage: S, node: impl IntoNode<N>)
    where
        V: Add<Output = V>,
    {
        use crate as peregrine;
        self.update(stage, |n| node! { i!(n) + i!(node) });
    }

    pub fn multiply<N: Node<Output = V> + 'static>(&mut self, stage: S, node: impl IntoNode<N>)
    where
        V: Mul<Output = V>,
    {
        use crate as peregrine;
        self.update(stage, |n| node! { i!(n) * i!(node) });
    }
}

impl<S: Ord + Copy + Send + Sync, V> IntoNode<OrderUpstreamFinder<(S, usize), V>> for &Stages<S, V>
where
    V: Send + Sync,
{
    fn into_node(self) -> OrderUpstreamFinder<(S, usize), V> {
        self.0.read_at_end()
    }
}

#[cfg(test)]
mod tests {
    use peregrine_macros::node;

    use crate::run;

    use super::*;
    use crate as peregrine;

    #[test]
    fn test_single() {
        let mut stages = Stages::<usize, _>::new(1);

        stages.update(5, |n| node! { i!(n) * 2 });
        assert_eq!(run(&stages), 2);

        stages.multiply(6, 3);
        stages.add(5, 10);
        assert_eq!(run(&stages), 36);

        stages.set(0, 2);
        assert_eq!(run(&stages), 42);
    }

    #[test]
    fn test_dependents() {
        let mut stages_a = Stages::<usize, _>::new(1);
        let mut stages_b = Stages::<usize, _>::new(2);

        stages_a.add(1, 5);
        stages_a.multiply(3, 2);
        stages_b.add(1, &stages_a);
        stages_b.add(2, 1);

        assert_eq!(run(&stages_b), 15);
        assert_eq!(run(&stages_a), 12);

        stages_a.set(2, 100);
        assert_eq!(run(&stages_b), 203);
        assert_eq!(run(&stages_a), 200);
    }
}
