#![allow(clippy::self_assignment)]

use peregrine::activity::Ops;
use peregrine::*;
use peregrine_macros::op;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};

#[derive(Hash)]
pub struct IncrementA;

impl<'o, M: Model<'o>> Activity<'o, M> for IncrementA {
    fn run(&'o self, mut ops: Ops<'_, 'o, M>) -> Result<Duration> {
        ops += op! {
            ref mut: a += 1;
        };

        Ok(Duration::ZERO)
    }
}

#[derive(Hash)]
pub struct IncrementB;

impl<'o, M: Model<'o>> Activity<'o, M> for IncrementB {
    fn run(&'o self, mut ops: Ops<'_, 'o, M>) -> Result<Duration> {
        ops += op! {
            ref mut: b += 1;
        };

        Ok(Duration::ZERO)
    }
}

#[derive(Hash)]
pub struct SetBToA;

impl<'o, M: Model<'o>> Activity<'o, M> for SetBToA {
    fn run(&'o self, mut ops: Ops<'_, 'o, M>) -> Result<Duration> {
        ops += op! {
            mut:b = ref:a;
        };

        Ok(Duration::ZERO)
    }
}

#[derive(Hash)]
pub struct SetAToB;

impl<'o, M: Model<'o>> Activity<'o, M> for SetAToB {
    fn run(&'o self, mut ops: Ops<'_, 'o, M>) -> Result<Duration> {
        ops += op! {
            mut:a = ref:b;
        };

        Ok(Duration::ZERO)
    }
}

#[derive(Hash)]
pub struct AddBToA;

impl<'o, M: Model<'o>> Activity<'o, M> for AddBToA {
    fn run(&'o self, mut ops: Ops<'_, 'o, M>) -> Result<Duration> {
        ops += op! {
            ref mut: a += ref:b;
        };

        Ok(Duration::ZERO)
    }
}

pub struct EvalCounter(Arc<AtomicU16>);

impl<'o, M: Model<'o>> Activity<'o, M> for EvalCounter {
    fn run(&'o self, mut ops: Ops<'_, 'o, M>) -> Result<Duration> {
        ops += op! {
            mut:a = ref:a;
            self.0.fetch_add(1, Ordering::SeqCst);
        };

        Ok(Duration::ZERO)
    }
}

impl Hash for EvalCounter {
    fn hash<H: Hasher>(&self, _state: &mut H) {}
}

impl EvalCounter {
    // Cargo test incorrectly warns that this function is not used.
    // It totally is, I don't know what its talking about.
    #[allow(unused)]
    pub fn new() -> (Self, Arc<AtomicU16>) {
        let counter = Arc::new(AtomicU16::new(0));
        (Self(counter.clone()), counter)
    }
}

model! {
    pub B {
        pub b: u32
    }
}
model! {
    pub AB {
        pub a: u32,
        ..B
    }
}

pub fn init_plan(session: &Session) -> Plan<AB> {
    session.new_plan(seconds(-1), initial_conditions! { a: 0, b: 0 })
}

pub fn seconds(s: i32) -> Time {
    Time::from_tai_seconds(s as f64)
}
