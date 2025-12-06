use std::sync::Arc;

use crate::{
    Data, IntoUpstream, Upstream,
    graph::series::{Series, SeriesProbe},
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
    pub fn new<U: Upstream<Output = O> + 'a>(default: impl IntoUpstream<U>) -> Self {
        Self {
            series: Series::new(default),
            counter: 0,
        }
    }

    pub fn set<U: Upstream<Output = O> + 'a>(
        &mut self,
        index: T,
        value: impl IntoUpstream<U>,
    ) -> Dense<T> {
        let index = Dense {
            index,
            order: self.counter,
        };
        self.series.set(index, value);
        self.counter += 1;
        index
    }

    pub fn get(&mut self, index: T) -> Arc<SeriesProbe<'a, Dense<T>, O>> {
        self.series.get(Dense { index, order: 0 })
    }

    pub fn get_inclusive(&mut self, index: T) -> Arc<SeriesProbe<'a, Dense<T>, O>> {
        self.series.get_inclusive(Dense {
            index,
            order: u64::MAX,
        })
    }

    pub fn remove(&mut self, index: Dense<T>) -> Option<Arc<dyn Upstream<Output = O> + 'a>> {
        self.series.remove(index)
    }

    pub fn mutate<U: Upstream<Output = O> + 'a, IU: IntoUpstream<U>>(
        &mut self,
        index: T,
        f: impl FnOnce(Arc<SeriesProbe<'a, Dense<T>, O>>) -> IU,
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

#[cfg(test)]
mod tests {
    use crate as desparrow;
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
