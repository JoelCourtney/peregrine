use std::{any::TypeId, ops::Deref};

use hifitime::Epoch;
use slotmap::{Key, SecondaryMap, new_key_type};

use crate::{
    graph::series::dense::DenseSeries,
    undo::{ErasedRecorder, Record, Undo, Undoer},
};

pub struct Plan<M: Undo> {
    model: Undoer<M>,
    activities: SecondaryMap<ActivityId, (Time, Box<dyn ErasedActivity>)>,
}

new_key_type! { pub struct ActivityId; }

impl<M: Undo> Plan<M> {
    pub fn new(model: M) -> Self {
        Self {
            model: Undoer::new(model),
            activities: SecondaryMap::new(),
        }
    }

    pub fn insert(&mut self, time: Time, activity: impl Activity<M> + 'static) -> ActivityId {
        let batch_id = self.model.update(|model| {
            activity.apply(time, model);
        });

        let activity_id = batch_id.data().into();
        self.activities.insert(activity_id, (time, Box::new(activity)));
        activity_id
    }

    pub fn remove(&mut self, id: ActivityId) {
        self.model.undo(id.data().into());
    }
}

impl<M: Undo> Deref for Plan<M> {
    type Target = M;

    fn deref(&self) -> &M {
        &self.model
    }
}

pub trait Activity<M: Undo>: ErasedActivity {
    fn apply(&self, time: Time, model: Record<M>);
}

#[typetag::serde(tag = "type")]
pub trait ErasedActivity {
    fn model_type_ids(&self) -> Vec<TypeId>;

    /// # Safety
    ///
    /// The caller must provide an instance of `Record<M>` where `TypeId::of::<M>() == model_type_id`.
    unsafe fn apply_by_id(&self, time: Time, model_type_id: TypeId, model: &mut dyn ErasedRecorder);
}

pub type Time = Epoch;
pub type Resource<T> = DenseSeries<'static, Time, T>;

#[cfg(test)]
mod tests {
    use desparrow_macros::activity;
    use hifitime::Duration;
    use serde::{Deserialize, Serialize};

    use crate::{Undo, run};
    use crate as desparrow;

    use super::*;
    
    #[derive(Undo)]
    struct Model {
        #[undo] sub_model: SubModel
    }
    
    #[derive(Undo)]
    struct SubModel {
        #[undo] x: Resource<i32>,
    }
    
    #[derive(Serialize, Deserialize)]
    struct MyActivity {
        value: i32,
    }
    
    #[activity(apply_to = { Model => model.sub_model })]
    impl Activity<SubModel> for MyActivity {
        fn apply(&self, time: Time, model: Record<SubModel>) {
            model.x.set(time, self.value);
        }
    }
    
    #[test]
    fn activity() {
        let mut plan = Plan::new(SubModel { x: Resource::new(0) });
        
        let plan_start = Time::from_tai_seconds(0.0);
        let id = plan.insert(plan_start, MyActivity { value: 42 });
        let result = plan.x.get(plan_start + Duration::from_seconds(1.0));
        
        assert_eq!(run(&result), 42);
        
        plan.remove(id);
        assert_eq!(run(&result), 0);
    }
    
    #[test]
    fn activity_on_sub_model() {
        let mut plan = Plan::new(Model { sub_model: SubModel { x: Resource::new(0) } });
        
        let plan_start = Time::from_tai_seconds(0.0);
        let id = plan.insert(plan_start, MyActivity { value: 42 });
        let result = plan.sub_model.x.get(plan_start + Duration::from_seconds(1.0));
        
        assert_eq!(run(&result), 42);
        
        plan.remove(id);
        assert_eq!(run(&result), 0);
    }
}
