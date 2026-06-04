pub mod dense;
pub mod resource;

use crate::{
    Data, Upstream,
    cache::Cache,
    data::evolving::Evolving,
    node::Node,
    shared_lock::{SharedLock, SharedKey},
};
use std::{
    collections::BTreeMap,
    ops::Bound,
    sync::{Arc, Weak},
};

pub struct Series<'a, T, O> {
    entries: SharedLock<SeriesEntries<'a, T, O>>,
}

struct SeriesEntries<'a, T, O> {
    default: ProbedUpstream<'a, T, O>,
    map: BTreeMap<T, ProbedUpstream<'a, T, O>>,
}

impl<T, O> Drop for SeriesEntries<'_, T, O> {
    fn drop(&mut self) {
        let replacement = BTreeMap::new();
        let actual = std::mem::replace(&mut self.map, replacement);
        for _ in actual.into_iter().rev() {}
    }
}

struct ProbedUpstream<'a, T, O> {
    upstream: Arc<dyn Upstream<Output = O> + 'a>,
    probes: Vec<WeakSeriesProbe<'a, T, O>>,
}

impl<T, O> Clone for ProbedUpstream<'_, T, O> {
    fn clone(&self) -> Self {
        ProbedUpstream {
            upstream: self.upstream.clone(),
            probes: self.probes.clone(),
        }
    }
}

enum WeakSeriesProbe<'a, T, O> {
    Constant(Weak<ConstantSeriesProbe<'a, T, O>>),
    Evolving(Weak<EvolvingSeriesProbe<'a, T, O>>),
    Sampling(Weak<SamplingSeriesProbe<'a, T, O>>),
}

impl<T, O> Clone for WeakSeriesProbe<'_, T, O> {
    fn clone(&self) -> Self {
        match self {
            WeakSeriesProbe::Constant(w) => WeakSeriesProbe::Constant(w.clone()),
            WeakSeriesProbe::Evolving(w) => WeakSeriesProbe::Evolving(w.clone()),
            WeakSeriesProbe::Sampling(w) => WeakSeriesProbe::Sampling(w.clone()),
        }
    }
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
            unreachable!("Expected ConstantSeriesProbe.")
        }
    }

    fn unwrap_evolving(self) -> Arc<EvolvingSeriesProbe<'a, T, O>> {
        if let StrongSeriesProbe::Evolving(c) = self {
            c
        } else {
            unreachable!("Expected EvolvingSeriesProbe.")
        }
    }

    fn unwrap_sampling(self) -> Arc<SamplingSeriesProbe<'a, T, O>> {
        if let StrongSeriesProbe::Sampling(c) = self {
            c
        } else {
            unreachable!("Expected SamplingSeriesProbe.")
        }
    }
}

pub struct ConstantSeriesProbe<'a, T, O> {
    at: T,
    inclusive: bool,
    upstream: SharedLock<Arc<dyn Upstream<Output = O> + 'a>>,
    cache: Cache<()>,
}

pub struct EvolvingSeriesProbe<'a, T, O> {
    inner: ConstantSeriesProbe<'a, T, O>,
    upstream_at: SharedLock<T>,
}

pub struct SamplingSeriesProbe<'a, T, O> {
    inner: ConstantSeriesProbe<'a, T, O>,
    upstream_at: SharedLock<T>,
}

impl<'a, T: Copy + Ord, O: Data> Series<'a, T, O> {
    pub fn new(default: impl Upstream<Output = O> + 'a) -> Self {
        Series {
            entries: SharedLock::new(SeriesEntries {
                default: ProbedUpstream {
                    upstream: Arc::new(default),
                    probes: vec![],
                },
                map: BTreeMap::default(),
            }),
        }
    }

    pub fn set_at(&self, index: T, upstream: impl Upstream<Output = O> + 'a) {
        let upstream = Arc::new(upstream) as Arc<dyn Upstream<Output = O>>;
        let mut shared_key = SharedKey::new();
        let SeriesEntries { default, map } = &mut *self.entries.write(&mut shared_key);
        let probed = map
            .range_mut(..=index)
            .next_back()
            .map_or(default, |(_, v)| v);

        let new_probes = probed
            .probes
            .extract_if(.., |weak| {
                let Some(probe) = weak.upgrade() else {
                    return false;
                };
                probe
                    .reconsider(index, &upstream, &mut shared_key)
                    .should_extract()
            })
            .collect();

        map.insert(
            index,
            ProbedUpstream {
                upstream,
                probes: new_probes,
            },
        );
    }

    pub fn remove(&self, index: T) -> Option<Arc<dyn Upstream<Output = O> + 'a>> {
        let mut shared_key = SharedKey::new();
        let SeriesEntries { default, map } = &mut *self.entries.write(&mut shared_key);
        if let Some(ProbedUpstream {
            upstream: old_node,
            probes,
        }) = map.remove(&index)
        {
            let (new_key, probed) = map
                .range_mut(..=index)
                .next_back()
                .map_or((None, default), |(key, value)| (Some(*key), value));
            for weak in probes {
                if let Some(probe) = weak.upgrade() {
                    probe.switch(new_key, &probed.upstream, &mut shared_key);
                    probed.probes.push(weak);
                }
            }
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
        let mut shared_key = SharedKey::new();
        let SeriesEntries { default, map } = self.entries.write(&mut shared_key);
        let bound = if inclusive {
            Bound::Included(index)
        } else {
            Bound::Excluded(index)
        };
        let (key, probed) = map
            .range_mut((Bound::Unbounded, bound))
            .next_back()
            .map_or((None, default), |(k, v)| (Some(*k), v));
        let probe = ConstantSeriesProbe {
            at: index,
            inclusive,
            upstream: SharedLock::new(probed.upstream.clone()),
            cache: Cache::new(),
        };
        let mapped_probe = mapper(key, probe);
        probed.probes.push(mapped_probe.downgrade());
        mapped_probe
    }

    pub fn get_at(&self, index: T) -> Node<Arc<ConstantSeriesProbe<'a, T, O>>> {
        Node(
            self.get_internal(index, false, |_, probe| {
                StrongSeriesProbe::Constant(Arc::new(probe))
            })
            .unwrap_constant(),
        )
    }

    pub fn get_at_inc(&self, index: T) -> Node<Arc<ConstantSeriesProbe<'a, T, O>>> {
        Node(
            self.get_internal(index, true, |_, probe| {
                StrongSeriesProbe::Constant(Arc::new(probe))
            })
            .unwrap_constant(),
        )
    }

    pub fn mutate_at<U: Upstream<Output = O> + 'a>(
        &self,
        index: T,
        f: impl FnOnce(Node<Arc<ConstantSeriesProbe<'a, T, O>>>) -> U,
    ) {
        let result = f(self.get_at(index));
        self.set_at(index, result);
    }
}

impl<'a, T: Copy + Ord, O: Evolving<T>> Series<'a, T, O> {
    pub fn sample_at(&self, index: T) -> Node<Arc<SamplingSeriesProbe<'a, T, O>>> {
        Node(self.get_internal(
            index,
            false,
            |key, probe| StrongSeriesProbe::Sampling(Arc::new(SamplingSeriesProbe {
                inner: probe,
                upstream_at: SharedLock::new(key.expect("Cannot sample from the default value of a Series. The default occurs at -infinity.")),
            })),
        ).unwrap_sampling())
    }

    pub fn sample_at_inc(&self, index: T) -> Node<Arc<SamplingSeriesProbe<'a, T, O>>> {
        Node(self.get_internal(
            index,
            true,
            |key, probe| StrongSeriesProbe::Sampling(Arc::new(SamplingSeriesProbe {
                inner: probe,
                upstream_at: SharedLock::new(key.expect("Cannot sample from the default value of a Series. The default occurs at -infinity."))
            }))
        ).unwrap_sampling())
    }

    pub fn evolve_at(&self, index: T) -> Node<Arc<EvolvingSeriesProbe<'a, T, O>>> {
        Node(self.get_internal(
            index,
            false,
            |key, probe| StrongSeriesProbe::Evolving(Arc::new(EvolvingSeriesProbe {
                inner: probe,
                upstream_at: SharedLock::new(key.expect("Cannot sample from the default value of a Series. The default occurs at -infinity."))
            }))
        ).unwrap_evolving ())
    }

    pub fn evolve_at_inc(&self, index: T) -> Node<Arc<EvolvingSeriesProbe<'a, T, O>>> {
        Node(self.get_internal(
            index,
            true,
            |key, probe| StrongSeriesProbe::Evolving(Arc::new(EvolvingSeriesProbe {
                inner: probe,
                upstream_at: SharedLock::new(key.expect("Cannot sample from the default value of a Series. The default occurs at -infinity."))
            }))
        ).unwrap_evolving())
    }

    pub fn mutate_evolve_at<U: Upstream<Output = O> + 'a>(
        &self,
        index: T,
        f: impl FnOnce(Node<Arc<EvolvingSeriesProbe<'a, T, O>>>) -> U,
    ) {
        let result = f(self.evolve_at(index));
        self.set_at(index, result);
    }

    pub fn mutate_sample_at<U: Upstream<Output = O> + 'a>(
        &self,
        index: T,
        f: impl FnOnce(Node<Arc<SamplingSeriesProbe<'a, T, O>>>) -> U,
    ) {
        let result = f(self.sample_at(index));
        self.set_at(index, result);
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
        upstream: &Arc<dyn Upstream<Output = O> + 'a>,
        shared_key: &mut SharedKey<'_>,
    ) -> MontyHall {
        use MontyHall::{Stay, Switch};

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

        self.switch(Some(new_key), upstream, shared_key);
        Switch
    }

    fn switch(
        &self,
        new_key: Option<T>,
        upstream: &Arc<dyn Upstream<Output = O> + 'a>,
        shared_key: &mut SharedKey,
    ) {
        let constant_probe = match self {
            StrongSeriesProbe::Constant(probe) => &**probe,
            StrongSeriesProbe::Evolving(e) => &e.inner,
            StrongSeriesProbe::Sampling(s) => &s.inner,
        };

        constant_probe.cache.invalidate();

        *constant_probe.upstream.write(shared_key) = upstream.clone();

        match (self, new_key) {
            (StrongSeriesProbe::Constant(_), _) => {}
            (StrongSeriesProbe::Evolving(e), Some(new_key)) => {
                *e.upstream_at.write(shared_key) = new_key;
            }
            (StrongSeriesProbe::Sampling(s), Some(new_key)) => {
                *s.upstream_at.write(shared_key) = new_key;
            }
            _ => panic!(
                "Cannot evolve or sample the default value of a Series. The default value occurs at -infinity."
            ),
        }
    }
}

impl<T: Send + Sync, O: Data> Upstream for ConstantSeriesProbe<'_, T, O> {
    type Output = O;

    fn request<'s>(&'s self, ctx: crate::Ctx<'_, '_, 's>, callback: crate::Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        let sender = self.cache.get_invalidator_sender();
        let callback = callback.map(|mut c| {
            c.push_sender(sender, false);
            c
        });
        self.upstream.read(ctx.key).request(ctx, callback);
    }
}

impl<T: PartialEq + Clone + Send + Sync + 'static, O: Evolving<T>> Upstream
    for EvolvingSeriesProbe<'_, T, O>
{
    type Output = O;

    fn request<'s>(&'s self, ctx: crate::Ctx<'_, '_, 's>, callback: crate::Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        let sender = self.inner.cache.get_invalidator_sender();
        let upstream_at = self.upstream_at.read(ctx.key).clone();
        let at = self.inner.at.clone();
        let callback = if upstream_at == at {
            callback.map(|mut c| {
                c.push_sender(sender, false);
                c
            })
        } else {
            callback.map(|mut c| {
                c.push_sender(sender, false);
                c.map(|v: O| v.evolve(upstream_at, at))
            })
        };
        self.inner.upstream.read(ctx.key).request(ctx, callback);
    }
}

impl<T: Clone + Send + Sync + 'static, O: Evolving<T>> Upstream for SamplingSeriesProbe<'_, T, O> {
    type Output = O::Sample;

    fn request<'s>(&'s self, ctx: crate::Ctx<'_, '_, 's>, callback: crate::Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        let sender = self.inner.cache.get_invalidator_sender();
        let upstream_at = self.upstream_at.read(ctx.key).clone();
        let at = self.inner.at.clone();
        let callback = callback.map(|mut c| {
            c.push_sender(sender, false);
            c.map(|v: O| v.sample(upstream_at, at))
        });
        self.inner.upstream.read(ctx.key).request(ctx, callback);
    }
}

impl<T: Copy + Ord, O: Upstream<Output = O> + Default + Data> Default for Series<'_, T, O> {
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
        let s = Series::default();
        s.set_at(5, 5);

        let probe_0 = s.get_at_inc(0);
        let probe_5 = s.get_at_inc(5);
        let probe_10 = s.get_at_inc(10);

        assert_eq!(run(&probe_0), 0);
        assert_eq!(run(&probe_5), 5);
        assert_eq!(run(&probe_10), 5);
    }

    #[test]
    fn set_exclusive() {
        let s = Series::new(0);
        s.set_at(5, 5);

        let probe_0 = s.get_at(0);
        let probe_5 = s.get_at(5);
        let probe_10 = s.get_at(10);

        assert_eq!(run(&probe_0), 0);
        assert_eq!(run(&probe_5), 0);
        assert_eq!(run(&probe_10), 5);

        s.set_at(7, 7);

        assert_eq!(run(&probe_0), 0);
        assert_eq!(run(&probe_5), 0);
        assert_eq!(run(&probe_10), 7);

        s.set_at(3, 3);

        assert_eq!(run(probe_0), 0);
        assert_eq!(run(probe_5), 3);
        assert_eq!(run(probe_10), 7);
    }

    #[test]
    fn remove() {
        let s = Series::new(0);
        s.set_at(3, 3);
        s.set_at(5, 5);

        let probe_4 = s.get_at(4);
        let probe_6 = s.get_at(6);

        assert_eq!(run(&probe_4), 3);
        assert_eq!(run(&probe_6), 5);

        let removed = s.remove(3);

        assert_eq!(run(removed), Some(3));
        assert_eq!(run(probe_4), 0);
        assert_eq!(run(probe_6), 5);
    }

    #[test]
    fn mutate() {
        let s = Series::new(0);
        s.set_at(2, 2);

        s.mutate_at(3, |p| op!(i!(p) * 2));

        assert_eq!(run(s.get_at_inc(3)), 4);

        s.remove(2);
        assert_eq!(run(s.get_at_inc(3)), 0);
    }

    #[test]
    fn mutate_overwrite() {
        let s = Series::new(0);
        s.set_at(2, 2);

        s.set_at(3, 10);
        s.mutate_at(3, |p| op!(i!(p) * 2));

        assert_eq!(run(s.get_at_inc(3)), 4);

        s.remove(2);
        assert_eq!(run(s.get_at_inc(3)), 0);
    }

    #[test]
    fn overwrite() {
        let s = Series::default();
        s.set_at(5, 5);

        let probe_6 = s.get_at_inc(5);

        assert_eq!(run(&probe_6), 5);

        s.set_at(5, 10);
        assert_eq!(run(&probe_6), 10);
    }

    #[test]
    fn evolve() {
        let s = Series::new(Polynomial::<2, i32, f64>::constant(1.0));
        s.set_at(0, Polynomial::<2, _, _>::new(1, 1.0, 2.0, 3.0));

        let probe = s.evolve_at(1);
        assert_eq!(run(&probe), Polynomial::<2, _, _>::new(1, 6.0, 5.0, 3.0));
    }

    #[test]
    fn sample() {
        let s = Series::new(Polynomial::<1, i32, f64>::constant(1.0));
        s.set_at(0, Polynomial::<1, _, _>::new(1, 1.0, 2.0));

        let probe = s.sample_at(5);
        assert_eq!(run(&probe), 11.0);

        s.set_at(2, Polynomial::<1, _, _>::new(1, -20.0, 5.0));

        assert_eq!(run(&probe), -5.0);
    }
}
