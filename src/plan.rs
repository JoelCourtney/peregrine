use std::{collections::HashMap, iter::{Empty, empty}};

use derive_more::Deref;
use crate::{Data, graph::series::dense::{Dense, DenseSeries}};

pub trait Model {
    type Recorder<'a>: IntoAnonIterator<Item = Self::RecordId> where Self: 'a;
    type RecordId;
    
    fn recorder(&mut self) -> Self::Recorder<'_>;
    fn remove_record(&mut self, id: Self::RecordId);
}

pub trait IntoAnonIterator {
    type Item;
    
    fn into_anon_iter(self) -> impl Iterator<Item = Self::Item>;
}

impl<T: IntoIterator> IntoAnonIterator for T {
    type Item = T::Item;
    
    fn into_anon_iter(self) -> impl Iterator<Item = Self::Item> {
        self.into_iter()
    }
}

pub struct Plan<M: Model> {
    model: M,
    activities: HashMap<ActivityId, RecordedActivity<M>>,
    activity_id_counter: u64,
}

impl<M: Model> Plan<M> {
    pub fn new(model: M) -> Self {
        Self {
            model,
            activities: HashMap::new(),
            activity_id_counter: 0,
        }
    }
    
    pub fn insert(&mut self, activity: impl Activity<M> + 'static) {
        let id = ActivityId {
            id: self.activity_id_counter,
        };
        self.activity_id_counter += 1;
        
        let recorder = self.model.recorder();
        activity.execute(&recorder);
        let records = recorder.into_anon_iter().collect();
        let activity = RecordedActivity {
            activity: Box::new(activity),
            records,
        };
        self.activities.insert(id, activity);
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ActivityId {
    id: u64,
}

struct RecordedActivity<M: Model> {
    activity: Box<dyn Activity<M>>,
    records: Vec<M::RecordId>
}

pub trait Activity<M: Model> {
    fn execute(&self, model: Record<M>);
}

pub type Record<'a, 'm, M> = &'a <M as Model>::Recorder<'m>;

#[derive(Deref)]
pub struct DataRecorder<'a, T>(&'a T);

pub enum Never {}

impl<'a, T> IntoIterator for DataRecorder<'a, T> {
    type Item = Never;
    type IntoIter = Empty<Never>;

    fn into_iter(self) -> Self::IntoIter {
        empty()
    }
}

impl<T: Data> Model for T {
    type Recorder<'a> = DataRecorder<'a, T> where Self: 'a;
    type RecordId = Never;
    
    fn recorder(&mut self) -> Self::Recorder<'_> {
        DataRecorder(self)
    }
    
    fn remove_record(&mut self, _id: Never) {
        unreachable!()
    }
    
}

pub struct DenseSeriesRecorder<'a, 'm, T, O> {
    series: &'a mut DenseSeries<'m, T, O>,
    records: Vec<Dense<T>>
}

impl<'a, 'm, T, O> IntoIterator for DenseSeriesRecorder<'a, 'm, T, O> {
    type Item = Dense<T>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.records.into_iter()
    }
}

impl<'m, T: Ord + Copy, O: Data> Model for DenseSeries<'m, T, O> {
    type Recorder<'a> = DenseSeriesRecorder<'a, 'm, T, O> where Self: 'a;
    type RecordId = Dense<T>;
    
    fn recorder(&mut self) -> Self::Recorder<'_> {
        DenseSeriesRecorder { series: self, records: vec![] }
    }
    
    fn remove_record(&mut self, id: Dense<T>) {
        self.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::series::dense::DenseSeries;

    use super::*;
    
    struct MyModel {
        x: DenseSeries<'static, u32, f32>,
        y: DenseSeries<'static, u32, f32>,
    }
    
    enum MyModelRecord {
        X(<DenseSeries<'static, u32, f32> as Model>::RecordId),
        Y(<DenseSeries<'static, u32, f32> as Model>::RecordId),
    }
    
    impl Model for MyModel {
        type Recorder<'a> = MyModelRecorder<'a>;
        type RecordId = MyModelRecord;
        
        fn recorder<'a>(&'a mut self) -> Self::Recorder<'a> {
            MyModelRecorder {
                x: self.x.recorder(),
                y: self.y.recorder(),
            }
        }
        
        fn remove_record(&mut self, id: MyModelRecord) {
            match id {
                MyModelRecord::X(id) => self.x.remove_record(id),
                MyModelRecord::Y(id) => self.y.remove_record(id),
            }
        }
    }
    
    struct MyModelRecorder<'a> {
        x: DenseSeriesRecorder<'a, 'static, u32, f32>,
        y: DenseSeriesRecorder<'a, 'static, u32, f32>,
    }
    
    impl IntoAnonIterator for MyModelRecorder<'_> {
        type Item = MyModelRecord;

        fn into_anon_iter(self) -> impl Iterator<Item = Self::Item> {
            self.x.into_iter().map(MyModelRecord::X)
                .chain(self.y.into_iter().map(MyModelRecord::Y))
        }
    }
    
    #[test]
    fn make_plan() {
        let model = MyModel {
            x: DenseSeries::new(5.0),
            y: DenseSeries::new(6.0),
        };
        
        let plan = Plan::new(model);
        
    }
}
