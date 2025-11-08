pub mod dense;

use crate::{IntoUpstream, Upstream, cache::Cache, data::Data, graph::NodeId};
use parking_lot::Mutex;
use std::{
    collections::BTreeMap,
    ops::Bound,
    sync::{Arc, Weak},
};

use super::Node;

pub struct Series<'a, T, O> {
    list: Mutex<BTreeMap<T, ProbedNode<'a, T, O>>>,
    default: (Arc<dyn Upstream<Output = O> + 'a>, ProbeList<'a, T, O>),
}

type ProbedNode<'a, T, O> = (Arc<dyn Upstream<Output = O> + 'a>, ProbeList<'a, T, O>);
type ProbeList<'a, T, O> = Vec<Weak<SeriesProbe<'a, T, O>>>;

pub struct SeriesProbe<'a, T, O> {
    node: Node,
    at: T,
    inclusive: bool,
    upstream: Mutex<Arc<dyn Upstream<Output = O> + 'a>>,
    cache: Cache<()>,
}

impl<'a, T: Copy + Ord, O: Data> Series<'a, T, O> {
    pub fn new<U: Upstream<Output = O> + 'a>(default: impl IntoUpstream<U>) -> Self {
        Series {
            list: Default::default(),
            default: (Arc::new(default.into_upstream()), ProbeList::new()),
        }
    }

    pub fn set<U: Upstream<Output = O> + 'a>(&mut self, index: T, upstream: impl IntoUpstream<U>) {
        let upstream = Arc::new(upstream.into_upstream()) as Arc<dyn Upstream<Output = O>>;
        let mut list = self.list.lock();
        let (prev_node, probes) = list
            .range_mut(..=index)
            .next_back()
            .map(|(_, v)| v)
            .unwrap_or(&mut self.default);

        let prev_node_id = prev_node.node_id();
        let new_probes = probes
            .extract_if(.., |weak| {
                let Some(probe) = weak.upgrade() else {
                    return false;
                };
                probe
                    .reconsider(index, prev_node_id, &upstream)
                    .should_extract()
            })
            .collect();

        list.insert(index, (upstream, new_probes));
    }

    pub fn remove(&mut self, index: T) -> Option<Arc<dyn Upstream<Output = O> + 'a>> {
        let mut list = self.list.lock();
        if let Some((old_node, probes)) = list.remove(&index) {
            let (prev_node, prev_probes) = list
                .range_mut(..index)
                .next_back()
                .map(|(_, v)| v)
                .unwrap_or(&mut self.default);
            let old_node_id = old_node.node_id();
            for weak in probes.into_iter() {
                if let Some(probe) = weak.upgrade() {
                    probe.switch(old_node_id, prev_node);
                    prev_probes.push(weak);
                }
            }
            Some(old_node)
        } else {
            None
        }
    }

    fn get_internal(&mut self, index: T, inclusive: bool) -> Arc<SeriesProbe<'a, T, O>> {
        let bound = if inclusive {
            Bound::Included(index)
        } else {
            Bound::Excluded(index)
        };
        let mut list = self.list.lock();
        let (target, probes) = list
            .range_mut((Bound::Unbounded, bound))
            .next_back()
            .map(|(_, v)| v)
            .unwrap_or(&mut self.default);
        let node = Node::new();
        node.add_edges(target.node_id());
        let probe = Arc::new(SeriesProbe {
            node,
            at: index,
            inclusive,
            upstream: Mutex::new(target.clone()),
            cache: Cache::new(),
        });
        probes.push(Arc::downgrade(&probe));
        probe
    }

    pub fn get(&mut self, index: T) -> Arc<SeriesProbe<'a, T, O>> {
        self.get_internal(index, false)
    }

    pub fn get_inclusive(&mut self, index: T) -> Arc<SeriesProbe<'a, T, O>> {
        self.get_internal(index, true)
    }

    pub fn mutate<U: Upstream<Output = O> + 'a, IU: IntoUpstream<U>>(
        &mut self,
        index: T,
        f: impl FnOnce(Arc<SeriesProbe<'a, T, O>>) -> IU,
    ) {
        let result = f(self.get(index));
        self.set(index, result);
    }
}

enum MontyHall {
    Stay,
    Switch,
}

impl MontyHall {
    fn should_extract(self) -> bool {
        match self {
            MontyHall::Stay => false,
            MontyHall::Switch => true,
        }
    }
}

impl<'a, T: Ord, O: Data> SeriesProbe<'a, T, O> {
    fn reconsider(
        &self,
        new_key: T,
        old_node_id: Option<NodeId>,
        upstream: &Arc<dyn Upstream<Output = O> + 'a>,
    ) -> MontyHall {
        use MontyHall::*;

        if self.at < new_key || (!self.inclusive && self.at == new_key) {
            return Stay;
        }

        self.switch(old_node_id, upstream);
        Switch
    }

    fn switch(&self, old_node_id: Option<NodeId>, upstream: &Arc<dyn Upstream<Output = O> + 'a>) {
        self.node.remove_edges(old_node_id);
        self.node.add_edges(upstream.node_id());

        self.cache.invalidate();

        *self.upstream.lock() = upstream.clone();
    }
}

impl<'a, T: Send + Sync, O: Data> Upstream for SeriesProbe<'a, T, O> {
    type Output = O;

    fn node_id(&self) -> Option<NodeId> {
        Some(self.node.id)
    }

    fn request<'s>(&self, ctx: crate::Ctx<'_, 's>, callback: crate::Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        let sender = self.cache.get_invalidator_sender();
        let callback = callback.map(|mut c| {
            c.push_sender(sender, false);
            c
        });
        self.upstream.lock().request(ctx, callback);
    }
}

impl<'a, T, O: Data + Default + Send> Default for Series<'a, T, O> {
    fn default() -> Self {
        Series {
            list: Default::default(),
            default: (
                Arc::new(O::default().into_upstream()) as Arc<dyn Upstream<Output = O>>,
                vec![],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate as peregrine;
    use crate::{graph::series::Series, op, run};

    #[test]
    fn set_inclusive() {
        let mut s = Series::default();
        s.set(5, 5);

        let probe_0 = s.get_inclusive(0);
        let probe_5 = s.get_inclusive(5);
        let probe_10 = s.get_inclusive(10);

        assert_eq!(run(&probe_0), 0);
        assert_eq!(run(&probe_5), 5);
        assert_eq!(run(&probe_10), 5);
    }

    #[test]
    fn set_exclusive() {
        let mut s = Series::new(0);
        s.set(5, 5);

        let probe_0 = s.get(0);
        let probe_5 = s.get(5);
        let probe_10 = s.get(10);

        assert_eq!(run(&probe_0), 0);
        assert_eq!(run(&probe_5), 0);
        assert_eq!(run(&probe_10), 5);

        s.set(7, 7);

        assert_eq!(run(&probe_0), 0);
        assert_eq!(run(&probe_5), 0);
        assert_eq!(run(&probe_10), 7);

        s.set(3, 3);

        assert_eq!(run(probe_0), 0);
        assert_eq!(run(probe_5), 3);
        assert_eq!(run(probe_10), 7);
    }

    #[test]
    fn remove() {
        let mut s = Series::new(0);
        s.set(3, 3);
        s.set(5, 5);

        let probe_4 = s.get(4);
        let probe_6 = s.get(6);

        assert_eq!(run(&probe_4), 3);
        assert_eq!(run(&probe_6), 5);

        let removed = s.remove(3);

        assert_eq!(run(removed), Some(3));
        assert_eq!(run(probe_4), 0);
        assert_eq!(run(probe_6), 5);
    }

    #[test]
    fn mutate() {
        let mut s = Series::new(0);
        s.set(2, 2);

        s.mutate(3, |p| op!(i!(p) * 2));

        assert_eq!(run(s.get_inclusive(3)), 4);

        s.remove(2);
        assert_eq!(run(s.get_inclusive(3)), 0);
    }

    #[test]
    fn mutate_overwrite() {
        let mut s = Series::new(0);
        s.set(2, 2);

        s.set(3, 10);
        s.mutate(3, |p| op!(i!(p) * 2));

        assert_eq!(run(s.get_inclusive(3)), 4);

        s.remove(2);
        assert_eq!(run(s.get_inclusive(3)), 0);
    }
}
