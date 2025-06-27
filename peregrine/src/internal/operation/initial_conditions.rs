use crate::internal::exec::ExecEnvironment;
use crate::internal::history::PeregrineDefaultHashBuilder;
use crate::internal::operation::{
    Continuation, Downstream, Node, OperationState, OperationStatus, Upstream,
};
use crate::internal::resource::ErasedResource;
use crate::internal::timeline::{Timelines, duration_to_epoch};
use crate::public::resource::{Data, Resource};
use anyhow::anyhow;
use hifitime::Duration;
use parking_lot::Mutex;
use rayon::Scope;
use std::collections::HashMap;
use std::hash::Hasher;

pub struct InitialConditions(HashMap<u64, Box<dyn ErasedResource>>);

impl Default for InitialConditions {
    fn default() -> Self {
        Self::new()
    }
}

impl InitialConditions {
    pub fn new() -> Self {
        Self(HashMap::new())
    }
    pub fn insert<R: Resource>(mut self, value: R::Data) -> Self {
        let value: WriteValue<R> = WriteValue(value);
        self.0.insert(value.id(), Box::new(value));
        self
    }
    pub fn take<R: Resource>(&mut self) -> Option<R::Data> {
        unsafe {
            self.0
                .remove(&R::ID)
                .map(|v| v.downcast_owned::<WriteValue<R>>().0)
        }
    }
}

struct WriteValue<R: Resource>(R::Data);

impl<R: Resource> ErasedResource for WriteValue<R> {
    fn id(&self) -> u64 {
        R::ID
    }
}

type InitialConditionState<'o, R> =
    OperationState<(u64, <<R as Resource>::Data as Data<'o>>::Read), (), &'o dyn Downstream<'o, R>>;

pub struct InitialConditionOp<'o, R: Resource> {
    value: R::Data,
    state: Mutex<InitialConditionState<'o, R>>,
    time: Duration,
}

impl<R: Resource> InitialConditionOp<'_, R> {
    pub fn new(time: Duration, value: R::Data) -> Self {
        Self {
            value,
            state: Default::default(),
            time,
        }
    }
}

impl<'o, R: Resource> Node<'o> for InitialConditionOp<'o, R> {
    fn insert_self(&'o self, _timelines: &Timelines<'o>) -> anyhow::Result<()> {
        unreachable!()
    }

    fn remove_self(&self, _timelines: &Timelines<'o>) -> anyhow::Result<()> {
        Err(anyhow!("Cannot remove initial conditions."))
    }
}

impl<'o, R: Resource + 'o> Upstream<'o, R> for InitialConditionOp<'o, R> {
    fn request<'s>(
        &'o self,
        continuation: Continuation<'o, R>,
        already_registered: bool,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's,
    {
        let mut state = self.state.lock();
        let result = match state.status {
            OperationStatus::Dormant => {
                let bytes = bincode::serde::encode_to_vec(&self.value, bincode::config::standard())
                    .expect("could not hash initial condition");
                let mut hasher = PeregrineDefaultHashBuilder::default();
                hasher.write(&bytes);
                let hash = hasher.finish();
                let output = (hash, self.value.to_read(duration_to_epoch(self.time)));
                state.status = OperationStatus::Done(Ok(output));
                output
            }
            OperationStatus::Done(o) => o.unwrap(),
            _ => unreachable!(),
        };

        if !already_registered {
            if let Some(d) = continuation.to_downstream() {
                state.downstreams.push(d);
            }
        }

        drop(state);

        continuation.run(Ok(result), scope, timelines, env.increment());
    }

    fn notify_downstreams(&self, time_of_change: Duration) {
        let mut state = self.state.lock();

        state
            .downstreams
            .retain(|d| d.clear_upstream(Some(time_of_change)));
    }

    fn register_downstream_early(&self, downstream: &'o dyn Downstream<'o, R>) {
        self.state.lock().downstreams.push(downstream);
    }

    fn request_grounding<'s>(
        &'o self,
        continuation: crate::internal::operation::grounding::GroundingContinuation<'o>,
        _already_registered: bool,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's,
    {
        continuation.run(Ok(self.time), scope, timelines, env.increment());
    }
}
