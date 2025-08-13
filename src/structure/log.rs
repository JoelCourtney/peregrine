use async_trait::async_trait;

use crate::{Exec, IntoNode, Node};

use super::{Order, OrderCollector};

pub struct Log<I: Ord + Copy, L: Ord + Copy, V>(
    Order<(I, L, usize), Box<dyn Iterator<Item = V> + Send + Sync>>,
);

impl<I: Ord + Copy, L: Ord + Copy + Send + Sync, V: Send + Sync + Clone + 'static> Log<I, L, V> {
    pub fn new() -> Self {
        Log(Order::<
            (I, L, usize),
            Box<dyn Iterator<Item = V> + Send + Sync>,
        >::new(None::<V>))
    }

    pub fn emit_many_at<N: Node<Output = Box<dyn Iterator<Item = V> + Send + Sync>> + 'static>(
        &mut self,
        t: I,
        l: L,
        log: impl IntoNode<N>,
    ) {
        let index = self
            .0
            .last_in_range((t, l, 0)..(t, l, usize::MAX))
            .map(|(_, _, index)| index);
        let index = if let Some(i) = index { i + 1 } else { 0 };
        self.0.write((t, l, index), log.into_node());
    }

    pub fn emit_many<N: Node<Output = Box<dyn Iterator<Item = V> + Send + Sync>> + 'static>(
        &mut self,
        t: I,
        log: impl IntoNode<N>,
    ) where
        L: Default,
    {
        let l = L::default();
        let index = self
            .0
            .last_in_range((t, l, 0)..(t, l, usize::MAX))
            .map(|(_, _, index)| index);
        let index = if let Some(i) = index { i + 1 } else { 0 };
        self.0.write((t, l, index), log.into_node());
    }

    pub fn emit_at<N: Node<Output = V> + 'static>(&mut self, t: I, l: L, log: impl IntoNode<N>) {
        let index = self
            .0
            .last_in_range((t, l, 0)..(t, l, usize::MAX))
            .map(|(_, _, index)| index);
        let index = if let Some(i) = index { i + 1 } else { 0 };
        self.0
            .write((t, l, index), SingleItemWrapper(log.into_node()));
    }

    pub fn emit<N: Node<Output = V> + 'static>(&mut self, t: I, log: impl IntoNode<N>)
    where
        L: Default,
    {
        let l = L::default();
        let index = self
            .0
            .last_in_range((t, l, 0)..(t, l, usize::MAX))
            .map(|(_, _, index)| index);
        let index = if let Some(i) = index { i + 1 } else { 0 };
        self.0.write((t, l, index), SingleItemWrapper(log.into_node()));
    }

    pub fn collect(&self) -> LogCollector<I, L, V> {
        LogCollector(self.0.collect_at_end(|_| true))
    }

    pub fn collect_level(&self, level: L) -> LogCollector<I, L, V>
    where
        L: 'static,
    {
        LogCollector(self.0.collect_at_end(move |(_, l, _)| l >= &level))
    }

    pub fn collect_level_only(&self, level: L) -> LogCollector<I, L, V>
    where
        L: 'static,
    {
        LogCollector(self.0.collect_at_end(move |(_, l, _)| l == &level))
    }
}

pub struct SingleItemWrapper<N: Node>(N);

#[async_trait]
impl<V: 'static + Send + Sync, N: Node<Output = V>> Node for SingleItemWrapper<N> {
    type Output = Box<dyn Iterator<Item = V> + Send + Sync>;

    async fn run<'s>(&self, ex: Exec<'s>) -> Self::Output {
        Box::new(std::iter::once(self.0.run(ex).await))
    }
}

impl<I: Ord + Copy, L: Ord + Copy + Send + Sync, V: Send + Sync + Clone + 'static> Default
    for Log<I, L, V>
{
    fn default() -> Self {
        Self::new()
    }
}

pub struct LogCollector<I: Ord + Copy, L: Ord + Copy, V>(
    OrderCollector<(I, L, usize), Box<dyn Iterator<Item = V> + Send + Sync>>,
);

impl<I: Ord + Copy + Send + Sync, L: Ord + Copy + Send + Sync, V: Send + Sync + Clone + 'static>
    IntoNode<LogCollector<I, L, V>> for &Log<I, L, V>
{
    fn into_node(self) -> LogCollector<I, L, V> {
        LogCollector(self.0.collect_at_end(|_| true))
    }
}

#[async_trait]
impl<I: Ord + Copy + Send + Sync, L: Ord + Copy + Send + Sync, V: Send + Sync + Clone + 'static>
    Node for LogCollector<I, L, V>
{
    type Output = Vec<LogEntry<I, L, V>>;

    async fn run<'s>(&self, ex: Exec<'s>) -> Self::Output {
        let v = self.0.run(ex).await;
        v.into_iter()
            .flat_map(|((i, l, _), v)| {
                v.map(move |v| LogEntry {
                    index: i,
                    level: l,
                    value: v,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LogEntry<I, L, V> {
    pub index: I,
    pub level: L,
    pub value: V,
}

pub struct IterWrapper<I: Clone + IntoIterator>(I);

#[async_trait]
impl<I: Send + Sync + Clone + IntoIterator> Node for IterWrapper<I>
where
    I::IntoIter: 'static + Send + Sync,
{
    type Output = Box<dyn Iterator<Item = I::Item> + Send + Sync>;

    async fn run<'s>(&self, _ex: Exec<'s>) -> Self::Output {
        Box::new(self.0.clone().into_iter())
    }
}

impl<I: Send + Sync + Clone + IntoIterator> IntoNode<IterWrapper<I>> for I
where
    I::IntoIter: 'static + Send + Sync,
{
    fn into_node(self) -> IterWrapper<I> {
        IterWrapper(self)
    }
}

#[cfg(test)]
mod tests {
    use crate::{structure::log::LogEntry, Exec};

    use super::Log;

    #[test]
    fn test_log() {
        let mut log = Log::<usize, (), &'static str>::new();

        log.emit(0, "Hello, world!");
        log.emit_many(1, None);
        log.emit_many(2, vec!["A", "B"]);
        
        itertools::assert_equal(Exec::run_blocking(&log), vec![
            LogEntry { index: 0, level: (), value: "Hello, world!" },
            LogEntry { index: 2, level: (), value: "A" },
            LogEntry { index: 2, level: (), value: "B" },
        ]);
    }
}
