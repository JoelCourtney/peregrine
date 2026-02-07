use std::{cell::Cell, rc::Rc, sync::Arc};

use derive_more::Deref;
use hifitime::Duration;

use crate::{
    Upstream,
    data::{Data, evolving::Evolving},
    graph::series::{
        ConstantSeriesProbe, EvolvingSeriesProbe, SamplingSeriesProbe,
        dense::{Dense, DenseSeries, DenseSeriesRecordKey, DenseSeriesRecorder},
    },
    node::Node,
    plan::{Chronological, ErasedChronoRecorder, Time},
    undo::{ErasedRecorder, IntoAnonIterator, Undo},
};

pub struct Resource<O> {
    series: DenseSeries<'static, Duration, O>,
}

impl<O: Data> Resource<O> {
    pub fn new(default: impl Upstream<Output = O> + 'static) -> Self {
        Self {
            series: DenseSeries::new(default),
        }
    }

    pub fn set_at(&self, index: Time, value: impl Upstream<Output = O> + 'static) -> Dense<Time> {
        self.series
            .set_at(index.to_tai_duration(), value)
            .map(Time::from_tai_duration)
    }

    pub fn get_at(
        &self,
        index: Time,
    ) -> Node<Arc<ConstantSeriesProbe<'static, Dense<Duration>, O>>> {
        self.series.get_at(index.to_tai_duration())
    }

    pub fn get_at_inc(
        &self,
        index: Time,
    ) -> Node<Arc<ConstantSeriesProbe<'static, Dense<Duration>, O>>> {
        self.series.get_at_inc(index.to_tai_duration())
    }

    pub fn remove(&self, index: Dense<Time>) -> Option<Arc<dyn Upstream<Output = O>>> {
        self.series.remove(index.map(|time| time.to_tai_duration()))
    }

    pub fn mutate_at<U: Upstream<Output = O> + 'static>(
        &self,
        index: Time,
        f: impl FnOnce(Node<Arc<ConstantSeriesProbe<'static, Dense<Duration>, O>>>) -> U,
    ) -> Dense<Time> {
        self.series
            .mutate_at(index.to_tai_duration(), f)
            .map(Time::from_tai_duration)
    }
}

impl<O: Evolving<Dense<Duration>>> Resource<O> {
    pub fn sample_at(
        &self,
        index: Time,
    ) -> Node<Arc<SamplingSeriesProbe<'static, Dense<Duration>, O>>> {
        self.series.sample_at(index.to_tai_duration())
    }

    pub fn sample_at_inc(
        &self,
        index: Time,
    ) -> Node<Arc<SamplingSeriesProbe<'static, Dense<Duration>, O>>> {
        self.series.sample_at_inc(index.to_tai_duration())
    }

    pub fn evolve_at(
        &self,
        index: Time,
    ) -> Node<Arc<EvolvingSeriesProbe<'static, Dense<Duration>, O>>> {
        self.series.evolve_at(index.to_tai_duration())
    }

    pub fn evolve_at_inc(
        &self,
        index: Time,
    ) -> Node<Arc<EvolvingSeriesProbe<'static, Dense<Duration>, O>>> {
        self.series.evolve_at_inc(index.to_tai_duration())
    }

    pub fn mutate_evolve_at<U: Upstream<Output = O> + 'static>(
        &self,
        index: Time,
        f: impl FnOnce(Node<Arc<EvolvingSeriesProbe<'static, Dense<Duration>, O>>>) -> U,
    ) -> Dense<Time> {
        self.series
            .mutate_evolve_at(index.to_tai_duration(), f)
            .map(Time::from_tai_duration)
    }

    pub fn mutate_sample_at<U: Upstream<Output = O> + 'static>(
        &self,
        index: Time,
        f: impl FnOnce(Node<Arc<SamplingSeriesProbe<'static, Dense<Duration>, O>>>) -> U,
    ) -> Dense<Time> {
        self.series
            .mutate_sample_at(index.to_tai_duration(), f)
            .map(Time::from_tai_duration)
    }
}

pub struct ResourceRecorder<'a, O> {
    series: DenseSeriesRecorder<'a, 'static, Duration, O>,
}

impl<O: Data> ResourceRecorder<'_, O> {
    pub fn get_at(
        &self,
        index: Time,
    ) -> Node<Arc<ConstantSeriesProbe<'static, Dense<Duration>, O>>> {
        self.series.get_at(index.to_tai_duration())
    }

    pub fn get_at_inc(
        &self,
        index: Time,
    ) -> Node<Arc<ConstantSeriesProbe<'static, Dense<Duration>, O>>> {
        self.series.get_at_inc(index.to_tai_duration())
    }

    pub fn set_at(
        &self,
        index: Time,
        value: impl Upstream<Output = O> + 'static,
    ) -> DenseSeriesRecordKey {
        self.series.set_at(index.to_tai_duration(), value)
    }

    pub fn remove(
        &self,
        key: DenseSeriesRecordKey,
    ) -> Option<Arc<dyn Upstream<Output = O> + 'static>> {
        self.series.remove(key)
    }

    pub fn mutate_at<U: Upstream<Output = O> + 'static>(
        &self,
        index: Time,
        f: impl FnOnce(Node<Arc<ConstantSeriesProbe<'static, Dense<Duration>, O>>>) -> U,
    ) -> DenseSeriesRecordKey {
        self.series.mutate_at(index.to_tai_duration(), f)
    }
}

impl<O: Evolving<Dense<Duration>>> ResourceRecorder<'_, O> {
    pub fn sample_at(
        &self,
        index: Time,
    ) -> Node<Arc<SamplingSeriesProbe<'static, Dense<Duration>, O>>> {
        self.series.sample_at(index.to_tai_duration())
    }

    pub fn sample_at_inc(
        &self,
        index: Time,
    ) -> Node<Arc<SamplingSeriesProbe<'static, Dense<Duration>, O>>> {
        self.series.sample_at_inc(index.to_tai_duration())
    }

    pub fn evolve_at(
        &self,
        index: Time,
    ) -> Node<Arc<EvolvingSeriesProbe<'static, Dense<Duration>, O>>> {
        self.series.evolve_at(index.to_tai_duration())
    }

    pub fn evolve_at_inc(
        &self,
        index: Time,
    ) -> Node<Arc<EvolvingSeriesProbe<'static, Dense<Duration>, O>>> {
        self.series.evolve_at_inc(index.to_tai_duration())
    }

    pub fn mutate_sample_at<U: Upstream<Output = O> + 'static>(
        &self,
        index: Time,
        f: impl FnOnce(Node<Arc<SamplingSeriesProbe<'static, Dense<Duration>, O>>>) -> U,
    ) -> DenseSeriesRecordKey {
        self.series.mutate_sample_at(index.to_tai_duration(), f)
    }

    pub fn mutate_evolve_at<U: Upstream<Output = O> + 'static>(
        &self,
        index: Time,
        f: impl FnOnce(Node<Arc<EvolvingSeriesProbe<'static, Dense<Duration>, O>>>) -> U,
    ) -> DenseSeriesRecordKey {
        self.series.mutate_evolve_at(index.to_tai_duration(), f)
    }
}

impl<O> ErasedRecorder for ResourceRecorder<'_, O> {}

impl<O> IntoAnonIterator for ResourceRecorder<'_, O> {
    type Item = Dense<Time>;

    fn into_anon_iter(self) -> impl Iterator<Item = Dense<Time>> {
        self.series
            .into_anon_iter()
            .map(|dense| dense.map(Time::from_tai_duration))
    }
}

impl<O: Data> Undo for Resource<O> {
    type Recorder<'a>
        = ResourceRecorder<'a, O>
    where
        Self: 'a;
    type RecordId = Dense<Time>;

    fn recorder(&mut self) -> Self::Recorder<'_> {
        ResourceRecorder {
            series: self.series.recorder(),
        }
    }

    fn remove_record(&mut self, id: Dense<Time>) {
        self.remove(id);
    }
}

#[derive(Deref)]
pub struct ResourceChronoRecorder<'r, 'i, O> {
    time_tracker: Rc<Cell<Time>>,
    #[deref]
    recorder: &'r ResourceRecorder<'i, O>,
}

impl<O: Data> ResourceChronoRecorder<'_, '_, O> {
    pub fn set(&self, value: impl Upstream<Output = O> + 'static) -> DenseSeriesRecordKey {
        self.recorder.set_at(self.time_tracker.get(), value)
    }

    pub fn mutate<U: Upstream<Output = O> + 'static>(
        &self,
        f: impl FnOnce(Node<Arc<ConstantSeriesProbe<'static, Dense<Duration>, O>>>) -> U,
    ) -> DenseSeriesRecordKey {
        self.recorder.mutate_at(self.time_tracker.get(), f)
    }

    pub fn get(&self) -> Node<Arc<ConstantSeriesProbe<'static, Dense<Duration>, O>>> {
        self.recorder.get_at(self.time_tracker.get())
    }

    pub fn get_inc(&self) -> Node<Arc<ConstantSeriesProbe<'static, Dense<Duration>, O>>> {
        self.recorder.get_at_inc(self.time_tracker.get())
    }
}

impl<O: Evolving<Dense<Duration>>> ResourceChronoRecorder<'_, '_, O> {
    pub fn sample(&self) -> Node<Arc<SamplingSeriesProbe<'static, Dense<Duration>, O>>> {
        self.recorder.sample_at(self.time_tracker.get())
    }

    pub fn sample_inc(&self) -> Node<Arc<SamplingSeriesProbe<'static, Dense<Duration>, O>>> {
        self.recorder.sample_at_inc(self.time_tracker.get())
    }

    pub fn evolve(&self) -> Node<Arc<EvolvingSeriesProbe<'static, Dense<Duration>, O>>> {
        self.recorder.evolve_at(self.time_tracker.get())
    }

    pub fn evolve_inc(&self) -> Node<Arc<EvolvingSeriesProbe<'static, Dense<Duration>, O>>> {
        self.recorder.evolve_at_inc(self.time_tracker.get())
    }

    pub fn mutate_sample<U: Upstream<Output = O> + 'static>(
        &self,
        f: impl FnOnce(Node<Arc<SamplingSeriesProbe<'static, Dense<Duration>, O>>>) -> U,
    ) -> DenseSeriesRecordKey {
        self.recorder.mutate_sample_at(self.time_tracker.get(), f)
    }

    pub fn mutate_evolve<U: Upstream<Output = O> + 'static>(
        &self,
        f: impl FnOnce(Node<Arc<EvolvingSeriesProbe<'static, Dense<Duration>, O>>>) -> U,
    ) -> DenseSeriesRecordKey {
        self.recorder.mutate_evolve_at(self.time_tracker.get(), f)
    }
}

impl<O> ErasedChronoRecorder for ResourceChronoRecorder<'_, '_, O> {}

impl<O: Data> Chronological for Resource<O> {
    type ChronoRecorder<'r, 'i>
        = ResourceChronoRecorder<'r, 'i, O>
    where
        Self: 'r + 'i,
        'i: 'r;

    fn chrono_recorder<'r, 'i>(
        time: &Rc<Cell<Time>>,
        recorder: &'r mut ResourceRecorder<'i, O>,
    ) -> Self::ChronoRecorder<'r, 'i>
    where
        'i: 'r,
    {
        ResourceChronoRecorder::<'r, 'i, O> {
            time_tracker: time.clone(),
            recorder,
        }
    }
}
