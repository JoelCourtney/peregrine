use crate::Time;
use crate::internal::history::History;
use crate::internal::macro_prelude::peregrine_grounding;
use crate::internal::operation::initial_conditions::InitialConditions;
use crate::public::Model;
use crate::public::plan::Plan;
use bumpalo_herd::Herd;
use parking_lot::RwLock;

#[derive(Default)]
pub struct Session {
    pub(crate) herd: Herd,
    pub(crate) history: RwLock<History>,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_history(self) -> History {
        self.history.into_inner()
    }

    pub fn new_plan<'o, M: Model<'o> + 'o>(
        &'o self,
        time: Time,
        initial_conditions: InitialConditions,
    ) -> anyhow::Result<Plan<'o, M>>
    where
        Self: 'o,
    {
        let mut history = self.history.write();
        history.init::<peregrine_grounding>();
        M::init_history(&mut history);
        drop(history);
        Plan::new(self, time, initial_conditions)
    }
}

impl From<History> for Session {
    fn from(history: History) -> Self {
        Self {
            history: RwLock::new(history),
            ..Self::default()
        }
    }
}
