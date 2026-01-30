pub mod dense;
// pub mod fuzzy;

use crate::{Data, Upstream, cache::Cache, data::evolving::Evolving, graph::NodeId, node::Node};
use parking_lot::Mutex;
use std::{
    collections::BTreeMap,
    ops::Bound,
    sync::{Arc, Weak},
};

use super::NodeTracker;

pub struct Series<'a, T, O> {
    node: NodeTracker,
    entries: Mutex<SeriesEntries<'a, T, O>>,
}

struct SeriesEntries<'a, T, O> {
    default: ProbedUpstream<'a, T, O>,
    map: BTreeMap<T, ProbedUpstream<'a, T, O>>,
}

struct ProbedUpstream<'a, T, O> {
    upstream: Arc<dyn Upstream<Output = O> + 'a>,
    probes: Vec<WeakSeriesProbe<'a, T, O>>,
}

enum WeakSeriesProbe<'a, T, O> {
    Constant(Weak<ConstantSeriesProbe<'a, T, O>>),
    Evolving(Weak<EvolvingSeriesProbe<'a, T, O>>),
    Sampling(Weak<SamplingSeriesProbe<'a, T, O>>),
}

impl<'a, T, O> WeakSeriesProbe<'a, T, O> {
    fn upgrade(&self) -> Option<StrongSeriesProbe<'a, T, O>> {
        match self {
            WeakSeriesProbe::Constant(w) => w.upgrade().map(|c| StrongSeriesProbe::Constant(c)),
            WeakSeriesProbe::Evolving(w) => w.upgrade().map(|e| StrongSeriesProbe::Evolving(e)),
            WeakSeriesProbe::Sampling(w) => w.upgrade().map(|s| StrongSeriesProbe::Sampling(s)),
        }
    }
}

enum StrongSeriesProbe<'a, T, O> {
    Constant(Arc<ConstantSeriesProbe<'a, T, O>>),
    Evolving(Arc<EvolvingSeriesProbe<'a, T, O>>),
    Sampling(Arc<SamplingSeriesProbe<'a, T, O>>),
}

impl<'a, T, O> StrongSeriesProbe<'a, T, O> {
    fn downgrade(&self) -> WeakSeriesProbe<'a, T, O> {
        match self {
            StrongSeriesProbe::Constant(w) => WeakSeriesProbe::Constant(Arc::downgrade(w)),
            StrongSeriesProbe::Evolving(w) => WeakSeriesProbe::Evolving(Arc::downgrade(w)),
            StrongSeriesProbe::Sampling(w) => WeakSeriesProbe::Sampling(Arc::downgrade(w)),
        }
    }

    fn unwrap_constant(self) -> Arc<ConstantSeriesProbe<'a, T, O>> {
        if let StrongSeriesProbe::Constant(c) = self {
            c
        } else {
            panic!("Expected ConstantSeriesProbe.")
        }
    }

    fn unwrap_evolving(self) -> Arc<EvolvingSeriesProbe<'a, T, O>> {
        if let StrongSeriesProbe::Evolving(c) = self {
            c
        } else {
            panic!("Expected EvolvingSeriesProbe.")
        }
    }

    fn unwrap_sampling(self) -> Arc<SamplingSeriesProbe<'a, T, O>> {
        if let StrongSeriesProbe::Sampling(c) = self {
            c
        } else {
            panic!("Expected SamplingSeriesProbe.")
        }
    }
}

pub struct ConstantSeriesProbe<'a, T, O> {
    node: NodeTracker,
    at: T,
    inclusive: bool,
    upstream: Mutex<Arc<dyn Upstream<Output = O> + 'a>>,
    cache: Cache<()>,
}

pub struct EvolvingSeriesProbe<'a, T, O> {
    inner: ConstantSeriesProbe<'a, T, O>,
    upstream_at: Mutex<T>,
}

pub struct SamplingSeriesProbe<'a, T, O> {
    inner: ConstantSeriesProbe<'a, T, O>,
    upstream_at: Mutex<T>,
}

impl<'a, T: Copy + Ord, O: Data> Series<'a, T, O> {
    pub fn new(default: impl Upstream<Output = O> + 'a) -> Self {
        let node = NodeTracker::new();
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

    pub fn set(&mut self, index: T, upstream: impl Upstream<Output = O> + 'a) {
        let upstream = Arc::new(upstream) as Arc<dyn Upstream<Output = O>>;
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
            let (new_key, probed) = map
                .range_mut(..=index)
                .next_back()
                .map(|(key, value)| (Some(*key), value))
                .unwrap_or((None, default));
            let old_node_id = old_node.node_id();
            for weak in probes.into_iter() {
                if let Some(probe) = weak.upgrade() {
                    probe.switch(old_node_id, new_key, &probed.upstream);
                    probed.probes.push(weak);
                }
            }
            self.node.remove_edges(old_node_id);
            Some(old_node)
        } else {
            None
        }
    }

    fn get_internal(
        &self,
        index: T,
        inclusive: bool,
        mapper: impl Fn(Option<T>, ConstantSeriesProbe<T, O>) -> StrongSeriesProbe<T, O>,
    ) -> StrongSeriesProbe<'a, T, O> {
        let bound = if inclusive {
            Bound::Included(index)
        } else {
            Bound::Excluded(index)
        };
        let SeriesEntries { default, map } = &mut *self.entries.lock();
        let (key, probed) = map
            .range_mut((Bound::Unbounded, bound))
            .next_back()
            .map(|(k, v)| (Some(*k), v))
            .unwrap_or((None, default));
        let node = NodeTracker::new();
        node.add_edges(probed.upstream.node_id());
        let probe = ConstantSeriesProbe {
            node,
            at: index,
            inclusive,
            upstream: Mutex::new(probed.upstream.clone()),
            cache: Cache::new(),
        };
        let mapped = mapper(key, probe);
        probed.probes.push(mapped.downgrade());
        mapped
    }

    pub fn get(&self, index: T) -> Node<Arc<ConstantSeriesProbe<'a, T, O>>> {
        Node(
            self.get_internal(index, false, |_, probe| {
                StrongSeriesProbe::Constant(Arc::new(probe))
            })
            .unwrap_constant(),
        )
    }

    pub fn get_inc(&self, index: T) -> Node<Arc<ConstantSeriesProbe<'a, T, O>>> {
        Node(
            self.get_internal(index, true, |_, probe| {
                StrongSeriesProbe::Constant(Arc::new(probe))
            })
            .unwrap_constant(),
        )
    }

    pub fn mutate<U: Upstream<Output = O> + 'a>(
        &mut self,
        index: T,
        f: impl FnOnce(Node<Arc<ConstantSeriesProbe<'a, T, O>>>) -> U,
    ) {
        let result = f(self.get(index));
        self.set(index, result);
    }
}

impl<'a, T: Copy + Ord, O: Evolving<T>> Series<'a, T, O> {
    pub fn sample(&self, index: T) -> Node<Arc<SamplingSeriesProbe<'a, T, O>>> {
        Node(self.get_internal(
            index,
            false,
            |key, probe| StrongSeriesProbe::Sampling(Arc::new(SamplingSeriesProbe {
                inner: probe,
                upstream_at: Mutex::new(key.expect("Cannot sample from the default value of a Series. The default occurs at -infinity.")),
            })),
        ).unwrap_sampling())
    }

    pub fn sample_inc(&self, index: T) -> Node<Arc<SamplingSeriesProbe<'a, T, O>>> {
        Node(self.get_internal(
            index,
            true,
            |key, probe| StrongSeriesProbe::Sampling(Arc::new(SamplingSeriesProbe {
                inner: probe,
                upstream_at: Mutex::new(key.expect("Cannot sample from the default value of a Series. The default occurs at -infinity."))
            }))
        ).unwrap_sampling())
    }

    pub fn evolve(&self, index: T) -> Node<Arc<EvolvingSeriesProbe<'a, T, O>>> {
        Node(self.get_internal(
            index,
            false,
            |key, probe| StrongSeriesProbe::Evolving(Arc::new(EvolvingSeriesProbe {
                inner: probe,
                upstream_at: Mutex::new(key.expect("Cannot sample from the default value of a Series. The default occurs at -infinity."))
            }))
        ).unwrap_evolving ())
    }

    pub fn evolve_inc(&self, index: T) -> Node<Arc<EvolvingSeriesProbe<'a, T, O>>> {
        Node(self.get_internal(
            index,
            true,
            |key, probe| StrongSeriesProbe::Evolving(Arc::new(EvolvingSeriesProbe {
                inner: probe,
                upstream_at: Mutex::new(key.expect("Cannot sample from the default value of a Series. The default occurs at -infinity."))
            }))
        ).unwrap_evolving())
    }

    pub fn mutate_evolve<U: Upstream<Output = O> + 'a>(
        &mut self,
        index: T,
        f: impl FnOnce(Node<Arc<EvolvingSeriesProbe<'a, T, O>>>) -> U,
    ) {
        let result = f(self.evolve(index));
        self.set(index, result);
    }

    pub fn mutate_sample<U: Upstream<Output = O> + 'a>(
        &mut self,
        index: T,
        f: impl FnOnce(Node<Arc<SamplingSeriesProbe<'a, T, O>>>) -> U,
    ) {
        let result = f(self.sample(index));
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

impl<'a, T: Ord, O: Data> StrongSeriesProbe<'a, T, O> {
    fn reconsider(
        &self,
        new_key: T,
        old_node_id: Option<NodeId>,
        upstream: &Arc<dyn Upstream<Output = O> + 'a>,
    ) -> MontyHall {
        use MontyHall::*;

        let constant_probe = match self {
            StrongSeriesProbe::Constant(probe) => &**probe,
            StrongSeriesProbe::Evolving(e) => &e.inner,
            StrongSeriesProbe::Sampling(s) => &s.inner,
        };

        if constant_probe.at < new_key
            || (!constant_probe.inclusive && constant_probe.at == new_key)
        {
            return Stay;
        }

        self.switch(old_node_id, Some(new_key), upstream);
        Switch
    }

    fn switch(
        &self,
        old_node_id: Option<NodeId>,
        new_key: Option<T>,
        upstream: &Arc<dyn Upstream<Output = O> + 'a>,
    ) {
        let constant_probe = match self {
            StrongSeriesProbe::Constant(probe) => &**probe,
            StrongSeriesProbe::Evolving(e) => &e.inner,
            StrongSeriesProbe::Sampling(s) => &s.inner,
        };

        constant_probe.node.remove_edges(old_node_id);
        constant_probe.node.add_edges(upstream.node_id());

        constant_probe.cache.invalidate();

        *constant_probe.upstream.lock() = upstream.clone();

        match (self, new_key) {
            (StrongSeriesProbe::Constant(_), _) => {}
            (StrongSeriesProbe::Evolving(e), Some(new_key)) => {
                *e.upstream_at.lock() = new_key;
            }
            (StrongSeriesProbe::Sampling(s), Some(new_key)) => {
                *s.upstream_at.lock() = new_key;
            }
            _ => panic!(
                "Cannot evolve or sample the default value of a Series. The default value occurs at -infinity."
            ),
        }
    }
}

impl<'a, T: Send + Sync, O: Data> Upstream for ConstantSeriesProbe<'a, T, O> {
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

impl<'a, T: PartialEq + Clone + Send + Sync + 'static, O: Evolving<T>> Upstream
    for EvolvingSeriesProbe<'a, T, O>
{
    type Output = O;

    fn node_id(&self) -> Option<NodeId> {
        Some(self.inner.node.id)
    }

    fn request<'s>(&self, ctx: crate::Ctx<'_, 's>, callback: crate::Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        let sender = self.inner.cache.get_invalidator_sender();
        let upstream_at = self.upstream_at.lock().clone();
        let at = self.inner.at.clone();
        let callback = if upstream_at != at {
            callback.map(|mut c| {
                c.push_sender(sender, false);
                c.map(|v: O| v.evolve(upstream_at, at))
            })
        } else {
            callback.map(|mut c| {
                c.push_sender(sender, false);
                c
            })
        };
        self.inner.upstream.lock().request(ctx, callback);
    }
}

impl<'a, T: Clone + Send + Sync + 'static, O: Evolving<T>> Upstream
    for SamplingSeriesProbe<'a, T, O>
{
    type Output = O::Sample;

    fn node_id(&self) -> Option<NodeId> {
        Some(self.inner.node.id)
    }

    fn request<'s>(&self, ctx: crate::Ctx<'_, 's>, callback: crate::Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        let sender = self.inner.cache.get_invalidator_sender();
        let upstream_at = self.upstream_at.lock().clone();
        let at = self.inner.at.clone();
        let callback = callback.map(|mut c| {
            c.push_sender(sender, false);
            c.map(|v: O| v.sample(upstream_at, at))
        });
        self.inner.upstream.lock().request(ctx, callback);
    }
}

impl<'a, T: Copy + Ord, O: Upstream<Output = O> + Default + Data> Default for Series<'a, T, O> {
    fn default() -> Self {
        Series::new(O::default())
    }
}

#[cfg(test)]
mod tests {
    use crate as peregrine;
    use crate::data::polynomial::Polynomial;
    use crate::{graph::series::Series, op, run};

    #[test]
    fn set_inclusive() {
        let mut s = Series::default();
        s.set(5, 5);

        let probe_0 = s.get_inc(0);
        let probe_5 = s.get_inc(5);
        let probe_10 = s.get_inc(10);

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

        assert_eq!(run(s.get_inc(3)), 4);

        s.remove(2);
        assert_eq!(run(s.get_inc(3)), 0);
    }

    #[test]
    fn mutate_overwrite() {
        let mut s = Series::new(0);
        s.set(2, 2);

        s.set(3, 10);
        s.mutate(3, |p| op!(i!(p) * 2));

        assert_eq!(run(s.get_inc(3)), 4);

        s.remove(2);
        assert_eq!(run(s.get_inc(3)), 0);
    }

    #[test]
    fn overwrite() {
        let mut s = Series::default();
        s.set(5, 5);

        let probe_6 = s.get_inc(5);

        assert_eq!(run(&probe_6), 5);

        s.set(5, 10);
        assert_eq!(run(&probe_6), 10);
    }

    #[test]
    fn evolve() {
        let mut s = Series::new(Polynomial::<2, i32, f64>::constant(1.0));
        s.set(0, Polynomial::<2, _, _>::new(1, 1.0, 2.0, 3.0));

        let probe = s.evolve(1);
        assert_eq!(run(&probe), Polynomial::<2, _, _>::new(1, 6.0, 5.0, 3.0));
    }

    #[test]
    fn sample() {
        let mut s = Series::new(Polynomial::<1, i32, f64>::constant(1.0));
        s.set(0, Polynomial::<1, _, _>::new(1, 1.0, 2.0));

        let probe = s.sample(5);
        assert_eq!(run(&probe), 11.0);

        s.set(2, Polynomial::<1, _, _>::new(1, -20.0, 5.0));

        assert_eq!(run(&probe), -5.0);
    }
}
