use crate::exec::ExecEnvironment;
use crate::history::PeregrineDefaultHashBuilder;
use crate::operation::{Continuation, Downstream, Node, OperationState, OperationStatus, Upstream};
use crate::resource::{Data, ErasedResource, Resource};
use crate::timeline::{Timelines, duration_to_epoch};
use anyhow::anyhow;
use hifitime::Duration;
use parking_lot::Mutex;
use rayon::Scope;
use std::collections::HashMap;
use std::hash::Hasher;

#[macro_export]
macro_rules! initial_conditions {
    ($($res:ident $(: $val:expr)?),*$(,)?) => {
        $crate::operation::initial_conditions::InitialConditions::new()
            $(.insert::<$res>(
                $crate::reexports::spez::spez! {
                    for x = ($res::Unit, $($val)?);
                    match ($res, <$res as $crate::resource::Resource>::Data) -> <$res as $crate::resource::Resource>::Data {
                        x.1
                    }
                    match<R: $crate::resource::Resource> (R,) where R::Data: Default -> R::Data {
                        Default::default()
                    }
                    match<T> T {
                        panic!("Initial condition must either be given a value or implement Default.")
                    }
                }
            ))*
    };
}

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
    fn insert_self<'s>(
        &'o self,
        _timelines: &'s Timelines<'o>,
        _scope: &Scope<'s>,
    ) -> anyhow::Result<()>
    where
        'o: 's,
    {
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
}
