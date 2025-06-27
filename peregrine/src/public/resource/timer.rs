use crate::public::resource::Data;
use crate::{MaybeHash, Time};
use hifitime::Duration;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Serialize, Deserialize, Debug, Copy, Clone, Default, Hash)]
pub struct Stopwatch {
    duration: Duration,
    running: bool,
}

impl Stopwatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self) {
        self.running = true;
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub fn reset(&mut self) {
        self.duration = Duration::ZERO;
        self.running = false;
    }

    pub fn elapsed(&self) -> Duration {
        self.duration
    }

    pub fn is_running(&self) -> bool {
        self.running
    }
}

impl Data<'_> for Stopwatch {
    type Read = (Stopwatch, Time);
    type Sample = Stopwatch;

    fn to_read(&self, written: Time) -> Self::Read {
        (*self, written)
    }

    fn from_read(read: Self::Read, now: Time) -> Self {
        let new_duration = if read.0.running {
            read.0.duration + (now - read.1)
        } else {
            read.0.duration
        };
        Stopwatch {
            duration: new_duration,
            running: read.0.running,
        }
    }

    fn sample(read: &Self::Read, now: Time) -> Self::Sample {
        Self::from_read(*read, now)
    }
}

impl MaybeHash for Stopwatch {
    fn is_hashable(&self) -> bool {
        true
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.hash(state);
    }
}
