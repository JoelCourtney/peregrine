#![doc(hidden)]

use crate::History;
use crate::operation::ObservedErrorOutput;
use crossbeam::queue::SegQueue;
use derive_more::Deref;
use std::cell::UnsafeCell;
use std::error::Error;

use std::fmt::{Display, Formatter};

pub const STACK_LIMIT: usize = 2000;

#[derive(Copy, Clone)]
pub struct ExecEnvironment<'s, 'o: 's> {
    pub history: &'o History,
    pub errors: &'s ErrorAccumulator,
    pub stack_counter: usize,
}

impl<'s, 'o> ExecEnvironment<'s, 'o> {
    pub fn increment(self) -> ExecEnvironment<'s, 'o> {
        Self {
            stack_counter: self.stack_counter + 1,
            ..self
        }
    }

    pub fn reset(self) -> ExecEnvironment<'s, 'o> {
        Self {
            stack_counter: 0,
            ..self
        }
    }
}

#[derive(Deref, Default)]
#[repr(transparent)]
pub struct UnsafeSyncCell<T>(UnsafeCell<T>);
unsafe impl<T> Sync for UnsafeSyncCell<T> {}

impl<T> UnsafeSyncCell<T> {
    pub fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }
}

#[derive(Default, Debug)]
pub struct ErrorAccumulator(SegQueue<anyhow::Error>);
impl ErrorAccumulator {
    pub fn push(&self, err: anyhow::Error) {
        if !err.is::<ObservedErrorOutput>() {
            self.0.push(err);
        }
    }

    pub fn into_vec(self) -> Vec<anyhow::Error> {
        self.0.into_iter().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Display for ErrorAccumulator {
    fn fmt(&self, _f: &mut Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl Error for ErrorAccumulator {}
