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

/// ```compile_fail
/// use serde::{Deserialize, Serialize};
/// use peregrine::{Undo, Chronological, activity, plan::{Activity, Resource, Time, Planner}};
/// use hifitime::Duration;
/// #[derive(Undo, Chronological)]
/// struct SubModel {
///     x: Resource<i32>,
/// }
/// #[derive(Serialize, Deserialize)]
/// struct SyncedModelActivity {
///     value: i32,
/// }
/// #[activity]
/// impl Activity<SubModel> for SyncedModelActivity {
///     fn apply(&self, mut m: Planner<SubModel>) {
///         m.x.set(self.value);
///         m.wait(Duration::from_seconds(2.0));
///
///         m.x.set(m.x.get());
///         todo!("Make the line above pass!");
///     }
/// }
/// ```
#[allow(unused)]
struct CompileFailTest;

#[cfg(test)]
mod tests {
    use peregrine_macros::{chronological, op, sync};
    use serde::{Deserialize, Serialize};

    use crate::node::Node;
    use crate::plan::{Resource, Time};
    use crate::{self as peregrine, Upstream};
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

    #[chronological]
    impl SubModel {
        #[allow(unused)]
        fn x_sq(&self, #[sync] at: Time) -> Node<impl Upstream<Output = i32> + use<>> {
            op! {
                let x = i!(self.x.get_at(at));
                x * x
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    struct MyActivity {
        value: i32,
    }

    #[derive(Serialize, Deserialize)]
    struct SyncActivity {
        value: i32,
    }

    #[derive(Serialize, Deserialize)]
    struct SyncedModelActivity {
        value: i32,
    }

    #[activity(apply_to = { Model => m.sub_model })]
    impl Activity<SubModel> for MyActivity {
        fn apply(&self, mut m: Planner<SubModel>) {
            let original = m.x.get();
            m.x.set(self.value);
            m.wait(Duration::from_seconds(1.0));
            m.x.set(original + 1);
        }
    }

    #[activity]
    impl Activity<SubModel> for SyncActivity {
        fn apply(&self, mut m: Planner<SubModel>) {
            let mut internal_counter = Resource::new(0);
            sync!(internal_counter: Resource<i32> => m);

            let original = m.x.get();
            m.x.set(self.value);
            m.wait(Duration::from_seconds(1.0));

            internal_counter.set(m.x.get() + 5);
            m.x.set(original + 1);

            m.wait(Duration::from_seconds(1.0));
            m.x.set(internal_counter.get() + 1);
        }
    }

    #[activity]
    impl Activity<SubModel> for SyncedModelActivity {
        fn apply(&self, mut m: Planner<SubModel>) {
            m.x.set(self.value);
            m.wait(Duration::from_seconds(2.0));
            let x_sq = m.x_sq();
            m.x.set(x_sq);
        }
    }

    #[test]
    fn activity() {
        let mut plan = Plan::new(SubModel {
            x: Resource::new(0),
        });

        let plan_start = Time::from_tai_seconds(0.0);
        let id = plan.insert(plan_start, MyActivity { value: 42 });
        let result1 = plan.x.get_at(Time::from_tai_seconds(1.0));
        let result2 = plan.x.get_at(Time::from_tai_seconds(10.0));

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
        let id = plan.insert(plan_start, MyActivity { value: 42 });
        let result1 = plan.sub_model.x.get_at(Time::from_tai_seconds(1.0));
        let result2 = plan.sub_model.x.get_at(Time::from_tai_seconds(10.0));

        assert_eq!(run(&result1), 42);
        assert_eq!(run(&result2), 1);

        plan.remove(id);
        assert_eq!(run(&result1), 0);
        assert_eq!(run(&result2), 0);
    }

    #[test]
    fn activity_with_internal_resource() {
        let mut plan = Plan::new(SubModel {
            x: Resource::new(0),
        });

        let plan_start = Time::from_tai_seconds(0.0);
        let id = plan.insert(plan_start, SyncActivity { value: 42 });
        let result1 = plan.x.get_at(Time::from_tai_seconds(1.0));
        let result2 = plan.x.get_at(Time::from_tai_seconds(10.0));

        assert_eq!(run(&result1), 42);
        assert_eq!(run(&result2), 48);

        plan.remove(id);
        assert_eq!(run(&result1), 0);
        assert_eq!(run(&result2), 0);
    }

    #[test]
    fn model_with_synced_function() {
        let mut plan = Plan::new(SubModel {
            x: Resource::new(0),
        });

        let plan_start = Time::from_tai_seconds(0.0);
        let id = plan.insert(plan_start, SyncedModelActivity { value: 3 });
        let result1 = plan.x.get_at(Time::from_tai_seconds(1.0));
        let result2 = plan.x.get_at(Time::from_tai_seconds(10.0));

        assert_eq!(run(&result1), 3);
        assert_eq!(run(&result2), 9);

        plan.remove(id);
        assert_eq!(run(&result1), 0);
        assert_eq!(run(&result2), 0);
    }
}
