use std::{any::TypeId, ops::Deref};

use slotmap::{Key, SecondaryMap, new_key_type};

use crate::undo::{ErasedRecorder, Record, Undo, UndoTracker};

pub struct Specification<M: Undo> {
    model: UndoTracker<M>,
    activities: SecondaryMap<ActionId, Box<dyn ErasedAction>>,
}

new_key_type! { pub struct ActionId; }

impl<M: Undo> Specification<M> {
    pub fn new(model: M) -> Self {
        Self {
            model: UndoTracker::new(model),
            activities: SecondaryMap::new(),
        }
    }

    pub fn insert(&mut self, action: impl Action<M> + 'static) -> ActionId {
        let batch_id = self.model.update(|model| {
            action.apply(model);
        });

        let action_id = batch_id.data().into();
        self.activities.insert(action_id, Box::new(action));
        action_id
    }

    pub fn remove(&mut self, id: ActionId) {
        self.model.undo(id.data().into());
    }
}

impl<M: Undo> Deref for Specification<M> {
    type Target = M;

    fn deref(&self) -> &M {
        &self.model
    }
}

pub trait Action<M: Undo>: ErasedAction {
    fn apply(&self, model: Record<M>);
}

#[typetag::serde(tag = "type")]
pub trait ErasedAction {
    fn model_type_ids(&self) -> Vec<TypeId>;

    /// # Safety
    ///
    /// The caller must provide an instance of `Record<M>` where `TypeId::of::<M>() == model_type_id`.
    unsafe fn apply_by_id(&self, model_type_id: TypeId, model: &mut dyn ErasedRecorder);
}

#[cfg(test)]
mod tests {
    use peregrine_macros::action;
    use serde::{Deserialize, Serialize};

    use crate as peregrine;
    use crate::plan::{Resource, Time};
    use crate::{Undo, run};

    use super::*;

    #[derive(Undo)]
    struct Model {
        sub_model: SubModel,
    }

    #[derive(Undo)]
    struct SubModel {
        x: Resource<i32>,
    }

    #[derive(Serialize, Deserialize)]
    struct MyAction {
        value: i32,
    }

    #[action(apply_to = { Model => model.sub_model })]
    impl Action<SubModel> for MyAction {
        fn apply(&self, model: Record<SubModel>) {
            let time = Time::from_tai_seconds(0.0);
            model.x.set_at(time, self.value);
        }
    }

    #[test]
    fn action() {
        let mut spec = Specification::new(SubModel {
            x: Resource::new(0),
        });

        let id = spec.insert(MyAction { value: 42 });
        let result = spec.x.get_at(Time::from_tai_seconds(1.0));

        assert_eq!(run(&result), 42);

        spec.remove(id);
        assert_eq!(run(&result), 0);
    }

    #[test]
    fn action_on_sub_model() {
        let mut spec = Specification::new(Model {
            sub_model: SubModel {
                x: Resource::new(0),
            },
        });

        let id = spec.insert(MyAction { value: 42 });
        let result = spec.sub_model.x.get_at(Time::from_tai_seconds(1.0));

        assert_eq!(run(&result), 42);

        spec.remove(id);
        assert_eq!(run(&result), 0);
    }
}
