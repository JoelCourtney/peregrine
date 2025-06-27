#![allow(clippy::self_assignment)]

use peregrine::anyhow::Result;
use peregrine::public::activity::Ops;
use peregrine::*;
use peregrine_macros::op;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};

#[derive(Hash, Serialize, Deserialize)]
pub struct IncrementA;

#[typetag::serde]
impl Activity for IncrementA {
    fn run(&self, mut ops: Ops) -> Result<Duration> {
        ops += op! {
            ref mut: a += 1;
        };

        Ok(Duration::ZERO)
    }
}

#[derive(Hash, Serialize, Deserialize)]
pub struct IncrementB;

#[typetag::serde]
impl Activity for IncrementB {
    fn run(&self, mut ops: Ops) -> Result<Duration> {
        ops += op! {
            ref mut: b += 1;
        };

        Ok(Duration::ZERO)
    }
}

#[derive(Hash, Serialize, Deserialize)]
pub struct SetBToA;

#[typetag::serde]
impl Activity for SetBToA {
    fn run(&self, mut ops: Ops) -> Result<Duration> {
        ops += op! {
            mut:b = ref:a;
        };

        Ok(Duration::ZERO)
    }
}

#[derive(Hash, Serialize, Deserialize)]
pub struct SetAToB;

#[typetag::serde]
impl Activity for SetAToB {
    fn run(&self, mut ops: Ops) -> Result<Duration> {
        ops += op! {
            mut:a = ref:b;
        };

        Ok(Duration::ZERO)
    }
}

#[derive(Hash, Serialize, Deserialize)]
pub struct AddBToA;

#[typetag::serde]
impl Activity for AddBToA {
    fn run(&self, mut ops: Ops) -> Result<Duration> {
        ops += op! {
            ref mut: a += ref:b;
        };

        Ok(Duration::ZERO)
    }
}

#[derive(Serialize, Deserialize)]
pub struct EvalCounter(HashableAtomicU16);

#[derive(Clone, Serialize, Deserialize)]
pub struct HashableAtomicU16(Arc<AtomicU16>);

#[typetag::serde]
impl Activity for EvalCounter {
    fn run<'o>(&'o self, mut ops: Ops<'_, 'o>) -> Result<Duration> {
        let counter = &self.0;
        ops += op! {
            mut:a = ref:a;
            counter.0.fetch_add(1, Ordering::SeqCst);
        };

        Ok(Duration::ZERO)
    }
}

impl Hash for HashableAtomicU16 {
    fn hash<H: Hasher>(&self, _state: &mut H) {}
}

impl EvalCounter {
    // Cargo test incorrectly warns that this function is not used.
    // It totally is, I don't know what its talking about.
    #[allow(unused)]
    pub fn new() -> (Self, Arc<AtomicU16>) {
        let counter = Arc::new(AtomicU16::new(0));
        (Self(HashableAtomicU16(counter.clone())), counter)
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

#[allow(unused)]
pub fn init_plan(session: &Session) -> Plan<AB> {
    session.new_plan(seconds(-1), initial_conditions! { a: 0, b: 0 })
}

pub fn seconds(s: i32) -> Time {
    Time::from_tai_seconds(s as f64)
}
