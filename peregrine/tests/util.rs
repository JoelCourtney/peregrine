#![allow(clippy::self_assignment)]

use peregrine::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};

resource!(pub a: u32);
resource!(pub b: u32);

pub struct IncrementA;
impl_activity! { for IncrementA
    @(start) a -> a {
        a += 1;
    }
    Duration::ZERO
}

pub struct IncrementB;
impl_activity! { for IncrementB
    @(start) b -> b {
        b += 1;
    }
    Duration::ZERO
}

pub struct SetBToA;

#[activity]
impl Activity for SetBToA {
    fn run(&self, start: Time) -> Duration {

    }
}
activity! { for SetBToA

    op! {
        fn my_duration_expression(t) {
            mut: delay = 2 * ref: a
        }
    }


    @(start) + (2 * ref: a) {
        mut: b = ref: a;
        ref mut: mutator += 2;
    }

    Duration::ZERO
}

pub struct SetAToB;
impl_activity! { for SetAToB
    @(start): b -> a {
        use capture;
        a = b;
    }
    Duration::ZERO
}

pub struct AddBToA;
impl_activity! { for AddBToA
    @(start) a, b -> a {
        a += b;
    }
    Duration::ZERO
}

pub struct EvalCounter(Arc<AtomicU16>);
impl_activity! { for EvalCounter
    @(start) a -> a {
        a = a;
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    Duration::ZERO
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
    pub AB(a, b)
}

pub fn init_plan(session: &Session) -> Plan<AB> {
    session.new_plan(seconds(-1), ABInitialConditions { a: 0, b: 0 })
}

pub fn seconds(s: i32) -> Time {
    Time::from_tai_seconds(s as f64)
}
