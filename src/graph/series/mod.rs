pub mod dense;
pub mod fuzzy;

use crate::{Data, IntoUpstream, Upstream, cache::Cache, graph::NodeId};
use parking_lot::Mutex;
use std::{
    collections::BTreeMap,
    ops::Bound,
    sync::{Arc, Weak},
};

use super::Node;

pub struct Series<'a, T, O> {
    node: Node,
    entries: Mutex<SeriesEntries<'a, T, O>>,
}

struct SeriesEntries<'a, T, O> {
    default: ProbedUpstream<'a, T, O>,
    map: BTreeMap<T, ProbedUpstream<'a, T, O>>,
}

struct ProbedUpstream<'a, T, O> {
    upstream: Arc<dyn Upstream<Output = O> + 'a>,
    probes: Vec<Weak<SeriesProbe<'a, T, O>>>,
}

pub struct SeriesProbe<'a, T, O> {
    node: Node,
    at: T,
    inclusive: bool,
    upstream: Mutex<Arc<dyn Upstream<Output = O> + 'a>>,
    cache: Cache<()>,
}

impl<'a, T: Copy + Ord, O: Data> Series<'a, T, O> {
    pub fn new<U: Upstream<Output = O> + 'a>(default: impl IntoUpstream<U>) -> Self {
        let node = Node::new();
        let default = default.into_upstream();
        node.add_edges(default.node_id());
        Series {
            entries: Mutex::new(SeriesEntries {
                default: ProbedUpstream {
                    upstream: Arc::new(default),
                    probes: vec![],
                },
                map: Default::default(),
            }),
            node,
        }
    }

    pub fn set<U: Upstream<Output = O> + 'a>(&mut self, index: T, upstream: impl IntoUpstream<U>) {
        let upstream = Arc::new(upstream.into_upstream()) as Arc<dyn Upstream<Output = O>>;
        let SeriesEntries { default, map } = &mut *self.entries.lock();
        let probed = map
            .range_mut(..=index)
            .next_back()
            .map(|(_, v)| v)
            .unwrap_or(default);

        let prev_node_id = probed.upstream.node_id();
        let new_probes = probed
            .probes
            .extract_if(.., |weak| {
                let Some(probe) = weak.upgrade() else {
                    return false;
                };
                probe
                    .reconsider(index, prev_node_id, &upstream)
                    .should_extract()
            })
            .collect();

        let new_node_id = upstream.node_id();
        let removed = map.insert(
            index,
            ProbedUpstream {
                upstream,
                probes: new_probes,
            },
        );
        if let Some(ProbedUpstream {
            upstream: old_node, ..
        }) = removed
        {
            self.node.remove_edges(old_node.node_id());
        }
        self.node.add_edges(new_node_id);
    }

    pub fn remove(&mut self, index: T) -> Option<Arc<dyn Upstream<Output = O> + 'a>> {
        let SeriesEntries { default, map } = &mut *self.entries.lock();
        if let Some(ProbedUpstream {
            upstream: old_node,
            probes,
        }) = map.remove(&index)
        {
            let probed = map
                .range_mut(..=index)
                .next_back()
                .map(|(_, v)| v)
                .unwrap_or(default);
            let old_node_id = old_node.node_id();
            for weak in probes.into_iter() {
                if let Some(probe) = weak.upgrade() {
                    probe.switch(old_node_id, &probed.upstream);
                    probed.probes.push(weak);
                }
            }
            self.node.remove_edges(old_node_id);
            Some(old_node)
        } else {
            None
        }
    }

    fn get_internal(&self, index: T, inclusive: bool) -> Arc<SeriesProbe<'a, T, O>> {
        let bound = if inclusive {
            Bound::Included(index)
        } else {
            Bound::Excluded(index)
        };
        let SeriesEntries { default, map } = &mut *self.entries.lock();
        let probed = map
            .range_mut((Bound::Unbounded, bound))
            .next_back()
            .map(|(_, v)| v)
            .unwrap_or(default);
        let node = Node::new();
        node.add_edges(probed.upstream.node_id());
        let probe = Arc::new(SeriesProbe {
            node,
            at: index,
            inclusive,
            upstream: Mutex::new(probed.upstream.clone()),
            cache: Cache::new(),
        });
        probed.probes.push(Arc::downgrade(&probe));
        probe
    }

    pub fn get(&self, index: T) -> Arc<SeriesProbe<'a, T, O>> {
        self.get_internal(index, false)
    }

    pub fn get_inclusive(&self, index: T) -> Arc<SeriesProbe<'a, T, O>> {
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

impl<'a, T: Copy + Ord, O: Data + Default + Send> Default for Series<'a, T, O> {
    fn default() -> Self {
        Series::new(O::default())
    }
}

#[cfg(test)]
mod tests {
    use crate as desparrow;
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

    #[test]
    fn overwrite() {
        let mut s = Series::default();
        s.set(5, 5);

        let probe_6 = s.get_inclusive(5);

        assert_eq!(run(&probe_6), 5);

        s.set(5, 10);
        assert_eq!(run(&probe_6), 10);
    }
}
