use crate as peregrine;
use crate::macro_prelude::{InitialConditionOp, MaybeHash};
use crate::resource::Data;
use crate::timeline::Timelines;
use crate::{Model, Time, resource};
use hifitime::Duration;
use serde::{Deserialize, Serialize};
use std::hash::Hasher;

pub(crate) fn init_builtins_timelines<'o, M: Model<'o>>(
    time: Duration,
    timelines: &mut Timelines<'o, M>,
) {
    timelines.init_for_resource(
        time,
        InitialConditionOp::<'o, now, M>::new(time, PeregrineTimeTracker),
    );
    timelines.init_for_resource(
        time,
        InitialConditionOp::<'o, elapsed_time, M>::new(time, PeregrineElapsedTimeTracker),
    );
}

resource!(
    pub now: PeregrineTimeTracker,
    pub elapsed_time: PeregrineElapsedTimeTracker,
);

#[derive(Serialize, Deserialize, Debug, Copy, Clone, Default)]
pub struct PeregrineTimeTracker;

impl MaybeHash for PeregrineTimeTracker {
    fn is_hashable(&self) -> bool {
        true
    }

    fn hash_unchecked<H: Hasher>(&self, _state: &mut H) {
        // this page intentionally left blank
    }
}

impl Data<'_> for PeregrineTimeTracker {
    type Read = ();
    type Sample = Time;

    fn to_read(&self, _written: Time) -> Self::Read {}

    fn from_read(_read: Self::Read, _now: Time) -> Self {
        PeregrineTimeTracker
    }

    fn sample(_read: &Self::Read, now: Time) -> Self::Sample {
        now
    }
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, Default)]
pub struct PeregrineElapsedTimeTracker;

impl MaybeHash for PeregrineElapsedTimeTracker {
    fn is_hashable(&self) -> bool {
        true
    }

    fn hash_unchecked<H: Hasher>(&self, _state: &mut H) {
        // this page intentionally left blank
    }
}

impl Data<'_> for PeregrineElapsedTimeTracker {
    type Read = Time;
    type Sample = Duration;

    fn to_read(&self, written: Time) -> Self::Read {
        written
    }

    fn from_read(_read: Self::Read, _now: Time) -> Self {
        panic!("You cannot write to the `elapsed_time` builtin. Use a Stopwatch.")
    }

    fn sample(written: &Self::Read, now: Time) -> Self::Sample {
        now - *written
    }
}
