use crate::Time;
use crate::macro_prelude::{Data, MaybeHash};
use hifitime::Duration;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Serialize, Deserialize, Debug, Copy, Clone, Default)]
pub struct Stopwatch(Option<Time>, Time);

impl Stopwatch {
    pub fn new() -> Self {
        Self(None, Time::default())
    }

    pub fn start(&mut self) {
        self.0 = Some(self.1);
    }

    pub fn stop(&mut self) {
        self.0 = None;
    }

    pub fn elapsed(&self) -> Option<Duration> {
        self.0.map(|start| self.1 - start)
    }

    pub fn is_running(&self) -> bool {
        self.0.is_some()
    }
}

impl MaybeHash for Stopwatch {
    fn is_hashable(&self) -> bool {
        true
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
        self.1.hash(state);
    }
}

impl Data<'_> for Stopwatch {
    type Read = Option<Time>;
    type Sample = Option<Duration>;

    fn to_read(&self, _written: Time) -> Self::Read {
        self.0
    }

    fn from_read(read: Self::Read, now: Time) -> Self {
        Self(read, now)
    }

    fn sample(read: &Self::Read, now: Time) -> Self::Sample {
        read.map(|start| now - start)
    }
}
