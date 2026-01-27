use std::sync::Arc;

use derive_more::Deref;
use slotmap::{SlotMap, new_key_type};

use crate::{
    Data, Upstream,
    graph::series::{Series, SeriesProbe},
    node::Node,
    undo::{ErasedRecorder, IntoAnonIterator, Undo},
};

#[derive(Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash, Debug)]
pub struct Dense<T> {
    index: T,
    order: u64,
}

pub struct DenseSeries<'a, T, O> {
    series: Series<'a, Dense<T>, O>,
    counter: u64,
}

impl<'a, T: Ord + Copy, O: Data> DenseSeries<'a, T, O> {
    pub fn new(default: impl Upstream<Output = O> + 'a) -> Self {
        Self {
            series: Series::new(default),
            counter: 0,
        }
    }

    pub fn set(&mut self, index: T, value: impl Upstream<Output = O> + 'a) -> Dense<T> {
        let index = Dense {
            index,
            order: self.counter,
        };
        self.series.set(index, value);
        self.counter += 1;
        index
    }

    pub fn get(&self, index: T) -> Node<Arc<SeriesProbe<'a, Dense<T>, O>>> {
        self.series.get(Dense { index, order: 0 })
    }

    pub fn get_inclusive(&self, index: T) -> Node<Arc<SeriesProbe<'a, Dense<T>, O>>> {
        self.series.get_inclusive(Dense {
            index,
            order: u64::MAX,
        })
    }

    pub fn remove(&mut self, index: Dense<T>) -> Option<Arc<dyn Upstream<Output = O> + 'a>> {
        self.series.remove(index)
    }

    pub fn mutate<U: Upstream<Output = O> + 'a>(
        &mut self,
        index: T,
        f: impl FnOnce(Node<Arc<SeriesProbe<'a, Dense<T>, O>>>) -> U,
    ) -> Dense<T> {
        let index = Dense {
            index,
            order: self.counter,
        };
        self.counter += 1;
        self.series.mutate(index, f);
        index
    }
}

new_key_type! { pub struct DenseSeriesRecordKey; }

#[derive(Deref)]
pub struct DenseSeriesRecorder<'a, 'm, T, O> {
    #[deref]
    series: &'a mut DenseSeries<'m, T, O>,
    records: SlotMap<DenseSeriesRecordKey, Dense<T>>,
}

impl<'a, T: Copy + Ord, O: Data> DenseSeriesRecorder<'_, 'a, T, O> {
    pub fn set(&mut self, index: T, value: impl Upstream<Output = O> + 'a) -> DenseSeriesRecordKey {
        let index = self.series.set(index, value);
        self.records.insert(index)
    }

    pub fn remove(
        &mut self,
        key: DenseSeriesRecordKey,
    ) -> Option<Arc<dyn Upstream<Output = O> + 'a>> {
        if let Some(index) = self.records.remove(key) {
            self.series.remove(index)
        } else {
            None
        }
    }

    pub fn mutate<U: Upstream<Output = O> + 'a>(
        &mut self,
        index: T,
        f: impl FnOnce(Node<Arc<SeriesProbe<'a, Dense<T>, O>>>) -> U,
    ) -> DenseSeriesRecordKey {
        let index = self.series.mutate(index, f);
        self.records.insert(index)
    }
}

impl<'a, 'm, T, O> ErasedRecorder for DenseSeriesRecorder<'a, 'm, T, O> {}

impl<'a, 'm, T, O> IntoAnonIterator for DenseSeriesRecorder<'a, 'm, T, O> {
    type Item = Dense<T>;

    fn into_anon_iter(self) -> impl Iterator<Item = Dense<T>> {
        self.records.into_iter().map(|(_, v)| v)
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
            records: SlotMap::with_key(),
        }
    }

    fn remove_record(&mut self, id: Dense<T>) {
        self.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use crate as peregrine;
    use crate::{graph::series::dense::DenseSeries, op, run};

    #[test]
    fn dense_set_remove() {
        let mut s = DenseSeries::new(0);
        s.set(1, 1);
        s.set(1, 2);
        let idx = s.set(1, 3);

        let probe_2 = s.get(2);

        assert_eq!(run(&probe_2), 3);

        s.remove(idx);

        assert_eq!(run(probe_2), 2);
    }

    #[test]
    fn dense_mutate() {
        let mut s = DenseSeries::new(0);
        s.set(1, 1);
        s.mutate(1, |p| op!(i!(p) + 1));
        let middle = s.mutate(1, |p| op!(i!(p) + 10));
        s.mutate(1, |p| op!(i!(p) + 1));

        assert_eq!(run(s.get(2)), 13);

        s.remove(middle);

        assert_eq!(run(s.get(2)), 3);
    }
}
