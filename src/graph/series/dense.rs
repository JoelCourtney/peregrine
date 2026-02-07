use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use derive_more::Deref;
use slotmap::{SlotMap, new_key_type};

use crate::{
    Data, Upstream,
    data::evolving::Evolving,
    graph::series::{ConstantSeriesProbe, EvolvingSeriesProbe, SamplingSeriesProbe, Series},
    node::Node,
    plan::{Chronological, ErasedChronoRecorder, Time},
    undo::{ErasedRecorder, IntoAnonIterator, Undo},
};

#[derive(Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash, Debug)]
pub struct Dense<T> {
    pub index: T,
    pub order: u64,
}

impl<T> Dense<T> {
    pub fn map<U>(self, f: impl Fn(T) -> U) -> Dense<U> {
        Dense {
            index: f(self.index),
            order: self.order,
        }
    }
}

pub struct DenseSeries<'a, T, O> {
    series: Series<'a, Dense<T>, O>,
    counter: AtomicU64,
}

impl<'a, T: Ord + Copy, O: Data> DenseSeries<'a, T, O> {
    pub fn new(default: impl Upstream<Output = O> + 'a) -> Self {
        Self {
            series: Series::new(default),
            counter: AtomicU64::new(0),
        }
    }

    pub fn set_at(&self, index: T, value: impl Upstream<Output = O> + 'a) -> Dense<T> {
        let index = Dense {
            index,
            order: self.counter.fetch_add(1, Ordering::Relaxed),
        };
        self.series.set_at(index, value);
        index
    }

    pub fn get_at(&self, index: T) -> Node<Arc<ConstantSeriesProbe<'a, Dense<T>, O>>> {
        self.series.get_at(Dense { index, order: 0 })
    }

    pub fn get_at_inc(&self, index: T) -> Node<Arc<ConstantSeriesProbe<'a, Dense<T>, O>>> {
        self.series.get_at_inc(Dense {
            index,
            order: u64::MAX,
        })
    }

    pub fn remove(&self, index: Dense<T>) -> Option<Arc<dyn Upstream<Output = O> + 'a>> {
        self.series.remove(index)
    }

    pub fn mutate_at<U: Upstream<Output = O> + 'a>(
        &self,
        index: T,
        f: impl FnOnce(Node<Arc<ConstantSeriesProbe<'a, Dense<T>, O>>>) -> U,
    ) -> Dense<T> {
        let index = Dense {
            index,
            order: self.counter.fetch_add(1, Ordering::Relaxed),
        };
        self.series.mutate_at(index, f);
        index
    }
}

impl<'a, T: Copy + Ord, O: Evolving<Dense<T>>> DenseSeries<'a, T, O> {
    pub fn sample_at(&self, index: T) -> Node<Arc<SamplingSeriesProbe<'a, Dense<T>, O>>> {
        self.series.sample_at(Dense { index, order: 0 })
    }

    pub fn sample_at_inc(&self, index: T) -> Node<Arc<SamplingSeriesProbe<'a, Dense<T>, O>>> {
        self.series.sample_at_inc(Dense {
            index,
            order: u64::MAX,
        })
    }

    pub fn evolve_at(&self, index: T) -> Node<Arc<EvolvingSeriesProbe<'a, Dense<T>, O>>> {
        self.series.evolve_at(Dense { index, order: 0 })
    }

    pub fn evolve_at_inc(&self, index: T) -> Node<Arc<EvolvingSeriesProbe<'a, Dense<T>, O>>> {
        self.series.evolve_at_inc(Dense {
            index,
            order: u64::MAX,
        })
    }

    pub fn mutate_evolve_at<U: Upstream<Output = O> + 'a>(
        &self,
        index: T,
        f: impl FnOnce(Node<Arc<EvolvingSeriesProbe<'a, Dense<T>, O>>>) -> U,
    ) -> Dense<T> {
        let index = Dense {
            index,
            order: self.counter.fetch_add(1, Ordering::Relaxed),
        };
        self.series.mutate_evolve_at(index, f);
        index
    }

    pub fn mutate_sample_at<U: Upstream<Output = O> + 'a>(
        &self,
        index: T,
        f: impl FnOnce(Node<Arc<SamplingSeriesProbe<'a, Dense<T>, O>>>) -> U,
    ) -> Dense<T> {
        let index = Dense {
            index,
            order: self.counter.fetch_add(1, Ordering::Relaxed),
        };
        self.series.mutate_sample_at(index, f);
        index
    }
}

new_key_type! { pub struct DenseSeriesRecordKey; }

#[derive(Deref)]
pub struct DenseSeriesRecorder<'a, 'm, T, O> {
    #[deref]
    series: &'a DenseSeries<'m, T, O>,
    records: RefCell<SlotMap<DenseSeriesRecordKey, Dense<T>>>,
}

impl<'a, T: Copy + Ord, O: Data> DenseSeriesRecorder<'_, 'a, T, O> {
    pub fn set_at(&self, index: T, value: impl Upstream<Output = O> + 'a) -> DenseSeriesRecordKey {
        let index = self.series.set_at(index, value);
        self.records.borrow_mut().insert(index)
    }

    pub fn remove(&self, key: DenseSeriesRecordKey) -> Option<Arc<dyn Upstream<Output = O> + 'a>> {
        if let Some(index) = self.records.borrow_mut().remove(key) {
            self.series.remove(index)
        } else {
            None
        }
    }

    pub fn mutate_at<U: Upstream<Output = O> + 'a>(
        &self,
        index: T,
        f: impl FnOnce(Node<Arc<ConstantSeriesProbe<'a, Dense<T>, O>>>) -> U,
    ) -> DenseSeriesRecordKey {
        let index = self.series.mutate_at(index, f);
        self.records.borrow_mut().insert(index)
    }
}

impl<'a, T: Copy + Ord, O: Evolving<Dense<T>>> DenseSeriesRecorder<'_, 'a, T, O> {
    pub fn mutate_sample_at<U: Upstream<Output = O> + 'a>(
        &self,
        index: T,
        f: impl FnOnce(Node<Arc<SamplingSeriesProbe<'a, Dense<T>, O>>>) -> U,
    ) -> DenseSeriesRecordKey {
        let index = self.series.mutate_sample_at(index, f);
        self.records.borrow_mut().insert(index)
    }

    pub fn mutate_evolve_at<U: Upstream<Output = O> + 'a>(
        &self,
        index: T,
        f: impl FnOnce(Node<Arc<EvolvingSeriesProbe<'a, Dense<T>, O>>>) -> U,
    ) -> DenseSeriesRecordKey {
        let index = self.series.mutate_evolve_at(index, f);
        self.records.borrow_mut().insert(index)
    }
}

impl<'a, 'm, T, O> ErasedRecorder for DenseSeriesRecorder<'a, 'm, T, O> {}

impl<'a, 'm, T, O> IntoAnonIterator for DenseSeriesRecorder<'a, 'm, T, O> {
    type Item = Dense<T>;

    fn into_anon_iter(self) -> impl Iterator<Item = Dense<T>> {
        self.records.into_inner().into_iter().map(|(_, v)| v)
    }
}

impl<'m, T: Ord + Copy, O: Data> Undo for DenseSeries<'m, T, O> {
    type Recorder<'a>
        = DenseSeriesRecorder<'a, 'm, T, O>
    where
        Self: 'a;
    type RecordId = Dense<T>;

    fn recorder(&mut self) -> Self::Recorder<'_> {
        DenseSeriesRecorder {
            series: self,
            records: RefCell::new(SlotMap::with_key()),
        }
    }

    fn remove_record(&mut self, id: Dense<T>) {
        self.remove(id);
    }
}

#[derive(Deref)]
pub struct DenseSeriesChronoRecorder<'r, 'i, 'm, O> {
    time_tracker: Rc<Cell<Time>>,
    #[deref]
    recorder: &'r DenseSeriesRecorder<'i, 'm, Time, O>,
}

impl<'m, O: Data> DenseSeriesChronoRecorder<'_, '_, 'm, O> {
    pub fn set(&self, value: impl Upstream<Output = O> + 'm) -> DenseSeriesRecordKey {
        self.recorder.set_at(self.time_tracker.get(), value)
    }

    pub fn mutate<U: Upstream<Output = O> + 'm>(
        &self,
        f: impl FnOnce(Node<Arc<ConstantSeriesProbe<'m, Dense<Time>, O>>>) -> U,
    ) -> DenseSeriesRecordKey {
        self.recorder.mutate_at(self.time_tracker.get(), f)
    }

    pub fn get(&self) -> Node<Arc<ConstantSeriesProbe<'m, Dense<Time>, O>>> {
        self.recorder.get_at(self.time_tracker.get())
    }

    pub fn get_inc(&self) -> Node<Arc<ConstantSeriesProbe<'m, Dense<Time>, O>>> {
        self.recorder.get_at_inc(self.time_tracker.get())
    }
}

impl<'m, O: Evolving<Dense<Time>>> DenseSeriesChronoRecorder<'_, '_, 'm, O> {
    pub fn sample(&self) -> Node<Arc<SamplingSeriesProbe<'m, Dense<Time>, O>>> {
        self.recorder.sample_at(self.time_tracker.get())
    }

    pub fn sample_inc(&self) -> Node<Arc<SamplingSeriesProbe<'m, Dense<Time>, O>>> {
        self.recorder.sample_at_inc(self.time_tracker.get())
    }

    pub fn evolve(&self) -> Node<Arc<EvolvingSeriesProbe<'m, Dense<Time>, O>>> {
        self.recorder.evolve_at(self.time_tracker.get())
    }

    pub fn evolve_inc(&self) -> Node<Arc<EvolvingSeriesProbe<'m, Dense<Time>, O>>> {
        self.recorder.evolve_at_inc(self.time_tracker.get())
    }

    pub fn mutate_sample<U: Upstream<Output = O> + 'm>(
        &self,
        f: impl FnOnce(Node<Arc<SamplingSeriesProbe<'m, Dense<Time>, O>>>) -> U,
    ) -> DenseSeriesRecordKey {
        self.recorder.mutate_sample_at(self.time_tracker.get(), f)
    }

    pub fn mutate_evolve<U: Upstream<Output = O> + 'm>(
        &self,
        f: impl FnOnce(Node<Arc<EvolvingSeriesProbe<'m, Dense<Time>, O>>>) -> U,
    ) -> DenseSeriesRecordKey {
        self.recorder.mutate_evolve_at(self.time_tracker.get(), f)
    }
}

impl<O> ErasedChronoRecorder for DenseSeriesChronoRecorder<'_, '_, '_, O> {}

impl<'m, O: Data> Chronological for DenseSeries<'m, Time, O> {
    type ChronoRecorder<'r, 'i>
        = DenseSeriesChronoRecorder<'r, 'i, 'm, O>
    where
        Self: 'r + 'i,
        'i: 'r;

    fn chrono_recorder<'r, 'i>(
        time: &Rc<Cell<Time>>,
        recorder: &'r mut DenseSeriesRecorder<'i, 'm, Time, O>,
    ) -> Self::ChronoRecorder<'r, 'i>
    where
        'i: 'r,
    {
        DenseSeriesChronoRecorder::<'r, 'i, 'm, O> {
            time_tracker: time.clone(),
            recorder,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate as peregrine;
    use crate::{graph::series::dense::DenseSeries, op, run};

    #[test]
    fn dense_set_remove() {
        let s = DenseSeries::new(0);
        s.set_at(1, 1);
        s.set_at(1, 2);
        let idx = s.set_at(1, 3);

        let probe_2 = s.get_at(2);

        assert_eq!(run(&probe_2), 3);

        s.remove(idx);

        assert_eq!(run(probe_2), 2);
    }

    #[test]
    fn dense_mutate() {
        let s = DenseSeries::new(0);
        s.set_at(1, 1);
        s.mutate_at(1, |p| op!(i!(p) + 1));
        let middle = s.mutate_at(1, |p| op!(i!(p) + 10));
        s.mutate_at(1, |p| op!(i!(p) + 1));

        assert_eq!(run(s.get_at(2)), 13);

        s.remove(middle);

        assert_eq!(run(s.get_at(2)), 3);
    }
}
