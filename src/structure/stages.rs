use std::{
    fmt::Debug,
    ops::{Add, Mul},
};

use forte::Worker;

use crate::{IntoNode, Node};

use super::{Order, OrderUpstreamFinder};

pub struct Stages<S: Ord + Copy, V>(Order<(S, usize), V>);

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
        self.update(stage, |n| AddNode::new(n, node));
    }

    pub fn multiply<N: Node<Output = V> + 'static>(&mut self, stage: S, node: impl IntoNode<N>)
    where
        V: Mul<Output = V>,
    {
        self.update(stage, |n| MulNode::new(n, node));
    }
}

struct AddNode<N: Node, M: Node>(N, M);

impl<N: Node, M: Node> AddNode<N, M> {
    fn new(n: impl IntoNode<N>, m: impl IntoNode<M>) -> Self {
        Self(n.into_node(), m.into_node())
    }
}

impl<N: Node, M: Node> Node for AddNode<N, M>
where
    N::Output: Add<M::Output>,
    <N::Output as Add<M::Output>>::Output: Send + Sync,
{
    type Output = <N::Output as Add<M::Output>>::Output;

    fn run(&self, s: &Worker) -> Self::Output {
        self.0.run(s) + self.1.run(s)
    }
}

struct MulNode<N: Node, M: Node>(N, M);

impl<N: Node, M: Node> MulNode<N, M> {
    fn new(n: impl IntoNode<N>, m: impl IntoNode<M>) -> Self {
        Self(n.into_node(), m.into_node())
    }
}

impl<N: Node, M: Node> Node for MulNode<N, M>
where
    N::Output: Mul<M::Output>,
    <N::Output as Mul<M::Output>>::Output: Send + Sync,
{
    type Output = <N::Output as Mul<M::Output>>::Output;

    fn run(&self, s: &Worker) -> Self::Output {
        self.0.run(s) * self.1.run(s)
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
    use crate::run;

    use super::*;

    #[test]
    fn test_single() {
        let mut stages = Stages::<usize, _>::new(1);

        stages.update(5, |n| AddNode::new(n.clone(), n));
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
