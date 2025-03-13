#![allow(clippy::self_assignment)]

use peregrine::*;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};

#[derive(Hash)]
pub struct IncrementA;
impl_activity! { for IncrementA
    @(start) {
        ref mut: a += 1;
    }
    Ok(Duration::ZERO)
}

#[derive(Hash)]
pub struct IncrementB;
impl_activity! { for IncrementB
    @(start) {
        ref mut: b += 1;
    }
    Ok(Duration::ZERO)
}

#[derive(Hash)]
pub struct SetBToA;
impl_activity! { for SetBToA
    @(start) {
        mut:b = ref:a;
    }
    Ok(Duration::ZERO)
}

#[derive(Hash)]
pub struct SetAToB;
impl_activity! { for SetAToB
    @(start) {
        mut:a = ref:b;
    }
    Ok(Duration::ZERO)
}

#[derive(Hash)]
pub struct AddBToA;
impl_activity! { for AddBToA
    @(start) {
        ref mut: a += ref:b;
    }
    Ok(Duration::ZERO)
}

pub struct EvalCounter(Arc<AtomicU16>);
impl_activity! { for EvalCounter
    @(start) {
        mut:a = ref:a;
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    Ok(Duration::ZERO)
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
