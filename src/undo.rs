use std::collections::HashMap;

use derive_more::Deref;

pub trait Undo {
    type Recorder<'a>: IntoAnonIterator<Item = Self::RecordId>
    where
        Self: 'a;
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

#[derive(Deref)]
pub struct Undoer<M: Undo> {
    #[deref]
    model: M,
    records: HashMap<BatchId, Vec<M::RecordId>>,
    batch_id_counter: u64,
}

impl<M: Undo> Undoer<M> {
    pub fn new(model: M) -> Self {
        Self {
            model,
            records: HashMap::new(),
            batch_id_counter: 0,
        }
    }

    pub fn update(&mut self, f: impl FnOnce(Record<M>)) -> BatchId {
        let id = BatchId {
            id: self.batch_id_counter,
        };
        self.batch_id_counter += 1;

        let mut recorder = self.model.recorder();
        f(&mut recorder);
        let records: Vec<M::RecordId> = recorder.into_anon_iter().collect();
        self.records.insert(id, records);
        id
    }

    pub fn undo(&mut self, id: BatchId) {
        if let Some(records) = self.records.remove(&id) {
            for record in records {
                self.model.remove_record(record);
            }
        }
    }

    pub fn into_inner(self) -> M {
        self.model
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BatchId {
    id: u64,
}

pub type Record<'a, 'm, M> = &'a mut <M as Undo>::Recorder<'m>;

pub enum Never {}

#[cfg(test)]
mod tests {
    use crate as desparrow;
    use crate::graph::series::dense::DenseSeries;
    use crate::{Undo, run};

    use super::*;

    #[derive(Undo)]
    struct MyModel {
        #[undo]
        x: DenseSeries<'static, u32, f32>,
        #[undo]
        y: DenseSeries<'static, u32, f32>,
        z: f32,
    }

    #[test]
    fn make_plan() {
        let model = MyModel {
            x: DenseSeries::new(5.0f32),
            y: DenseSeries::new(6.0f32),
            z: 10.0,
        };

        let mut model = Undoer::new(model);

        let id = model.update(|model| {
            let start = 5;
            let end = start + 3;

            model.x.set(start, *model.z);
            model.x.set(end, model.x.get(start));

            model.y.mutate(end, |y| y + model.x.get(end));
        });

        assert_eq!(run(model.x.get(4)), 5.0);
        assert_eq!(run(model.x.get(6)), 10.0);
        assert_eq!(run(model.y.get(10)), 16.0);
        assert_eq!(run(model.x.get(10)), 5.0);

        model.undo(id);

        assert_eq!(run(model.x.get(4)), 5.0);
        assert_eq!(run(model.x.get(6)), 5.0);
        assert_eq!(run(model.y.get(10)), 6.0);
        assert_eq!(run(model.x.get(10)), 5.0);
    }
}
