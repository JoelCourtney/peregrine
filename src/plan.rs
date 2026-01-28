use std::{any::TypeId, cell::Cell, rc::Rc};

use derive_more::{Deref, DerefMut};
use hifitime::Duration;
use slotmap::{Key, SecondaryMap, new_key_type};

use crate::{
    graph::series::dense::DenseSeries,
    undo::{Undo, UndoTracker},
};

pub type Time = hifitime::Epoch;
pub type Resource<T> = DenseSeries<'static, Time, T>;

new_key_type! { pub struct ActivityId; }

pub struct Plan<M: Chronological> {
    model: UndoTracker<M>,
    activities: SecondaryMap<ActivityId, (Time, Box<dyn ErasedActivity>)>,
}

impl<M: Chronological> std::ops::Deref for Plan<M> {
    type Target = M;

    fn deref(&self) -> &Self::Target {
        &self.model
    }
}

impl<M: Chronological> Plan<M> {
    pub fn new(model: M) -> Self {
        Self {
            model: UndoTracker::new(model),
            activities: SecondaryMap::new(),
        }
    }

    pub fn insert(&mut self, time: Time, activity: impl Activity<M> + 'static) -> ActivityId {
        let shared_time = Rc::new(Cell::new(time));
        let batch_id = self.model.update(|model| {
            let mut rec = M::chrono_recorder(&shared_time, model);
            activity.apply(Planner::from_raw_parts(shared_time, &mut rec));
        });

        let action_id = batch_id.data().into();
        self.activities
            .insert(action_id, (time, Box::new(activity)));
        action_id
    }

    pub fn remove(&mut self, id: ActivityId) {
        self.model.undo(id.data().into());
    }
}

pub trait Activity<M: Chronological>: ErasedActivity {
    fn apply(&self, model: Planner<M>);
}

#[typetag::serde(tag = "type")]
pub trait ErasedActivity {
    fn model_type_ids(&self) -> Vec<TypeId>;

    /// # Safety
    ///
    /// The caller must provide an instance of `Record<M>` where `TypeId::of::<M>() == model_type_id`.
    unsafe fn apply_by_id(
        &self,
        model_type_id: TypeId,
        time_tracker: Rc<Cell<Time>>,
        model: &mut dyn ErasedChronoRecorder,
    );
}

pub trait Chronological: Undo {
    type ChronoRecorder<'r, 'i>: ErasedChronoRecorder
    where
        Self: 'r + 'i,
        'i: 'r;

    fn chrono_recorder<'r, 'i>(
        time: &Rc<Cell<Time>>,
        recorder: &'r mut Self::Recorder<'i>,
    ) -> Self::ChronoRecorder<'r, 'i>
    where
        'i: 'r;
}

pub trait ErasedChronoRecorder {}

#[derive(Deref, DerefMut)]
pub struct Planner<'a, 'r, 'i, M: Chronological + 'r + 'i>
where
    'i: 'r,
{
    #[deref]
    #[deref_mut]
    recorder: &'a mut M::ChronoRecorder<'r, 'i>,
    time_tracker: Rc<Cell<Time>>,
}

impl<'a, 'i, 'm, M: Chronological> Planner<'a, 'i, 'm, M>
where
    'm: 'i,
{
    pub fn from_raw_parts(
        time_tracker: Rc<Cell<Time>>,
        recorder: &'a mut M::ChronoRecorder<'i, 'm>,
    ) -> Self {
        Self {
            recorder,
            time_tracker,
        }
    }

    pub fn map_model<N: Chronological>(
        self,
        f: impl FnOnce(&'a mut M::ChronoRecorder<'i, 'm>) -> &'a mut N::ChronoRecorder<'i, 'm>,
    ) -> Planner<'a, 'i, 'm, N> {
        let recorder = f(self.recorder);
        Planner {
            recorder,
            time_tracker: self.time_tracker,
        }
    }

    pub fn wait(&self, duration: Duration) {
        self.time_tracker.set(self.time_tracker.get() + duration);
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use crate as peregrine;
    use crate::plan::{Resource, Time};
    use crate::{Chronological, Undo, activity, run};

    use super::*;

    #[derive(Undo, Chronological)]
    struct Model {
        sub_model: SubModel,
    }

    #[derive(Undo, Chronological)]
    struct SubModel {
        x: Resource<i32>,
    }

    #[derive(Serialize, Deserialize)]
    struct MyAction {
        value: i32,
    }

    #[activity(apply_to = { Model => m.sub_model })]
    impl Activity<SubModel> for MyAction {
        fn apply(&self, mut m: Planner<SubModel>) {
            let original = m.x.get();
            m.x.set(self.value);
            m.wait(Duration::from_seconds(1.0));
            m.x.set(original + 1);
        }
    }

    #[test]
    fn activity() {
        let mut plan = Plan::new(SubModel {
            x: Resource::new(0),
        });

        let plan_start = Time::from_tai_seconds(0.0);
        let id = plan.insert(plan_start, MyAction { value: 42 });
        let result1 = plan.x.get(Time::from_tai_seconds(1.0));
        let result2 = plan.x.get(Time::from_tai_seconds(10.0));

        assert_eq!(run(&result1), 42);
        assert_eq!(run(&result2), 1);

        plan.remove(id);
        assert_eq!(run(&result1), 0);
        assert_eq!(run(&result2), 0);
    }

    #[test]
    fn activity_on_sub_model() {
        let mut plan = Plan::new(Model {
            sub_model: SubModel {
                x: Resource::new(0),
            },
        });

        let plan_start = Time::from_tai_seconds(0.0);
        let id = plan.insert(plan_start, MyAction { value: 42 });
        let result1 = plan.sub_model.x.get(Time::from_tai_seconds(1.0));
        let result2 = plan.sub_model.x.get(Time::from_tai_seconds(10.0));

        assert_eq!(run(&result1), 42);
        assert_eq!(run(&result2), 1);

        plan.remove(id);
        assert_eq!(run(&result1), 0);
        assert_eq!(run(&result2), 0);
    }
}
