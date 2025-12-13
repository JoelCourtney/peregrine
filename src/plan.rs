use std::any::TypeId;

use derive_more::Deref;
use hifitime::Epoch;
use slotmap::{Key, SecondaryMap, new_key_type};

use crate::{
    graph::series::dense::DenseSeries,
    undo::{ErasedRecorder, Record, Undo, Undoer},
};

#[derive(Deref)]
pub struct Plan<M: Undo> {
    #[deref]
    model: Undoer<M>,
    activities: SecondaryMap<ActivityId, Box<dyn ErasedActivity>>,
}

new_key_type! { pub struct ActivityId; }

impl<M: Undo> Plan<M> {
    pub fn insert(&mut self, activity: impl Activity<M> + 'static) -> ActivityId {
        let batch_id = self.model.update(|model| {
            activity.apply(model);
        });

        let activity_id = batch_id.data().into();
        self.activities.insert(activity_id, Box::new(activity));
        activity_id
    }

    pub fn remove(&mut self, id: ActivityId) {
        self.model.undo(id.data().into());
    }
}

pub trait Activity<M: Undo>: ErasedActivity {
    fn apply(&self, model: Record<M>);
}

pub trait ErasedActivity {
    fn model_type_ids(&self) -> Vec<TypeId>;

    /// # Safety
    ///
    /// The caller must provide an instance of `Record<M>` where `TypeId::of::<M>() == model_type_id`.
    unsafe fn apply_by_id(&self, model_type_id: TypeId, model: &mut dyn ErasedRecorder);
}

pub type Resource<T> = DenseSeries<'static, Epoch, T>;
