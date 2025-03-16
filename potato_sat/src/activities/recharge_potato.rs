use crate::{int_pieces, timer};
use peregrine::activity::*;
use peregrine::macro_prelude::peregrine_macros::op;
use peregrine::reexports::hifitime::TimeUnits;
use peregrine::resource::builtins::{elapsed, now};
use peregrine::resource::polynomial::Linear;
use peregrine::{Duration, Result, pieces};
use serde::{Deserialize, Serialize};

#[derive(Hash, Serialize, Deserialize)]
pub struct RechargePotato {
    pub amount: u32,
}

impl Activity for RechargePotato {
    fn run(&self, mut ops: Ops) -> Result<Duration> {
        let duration = 100.seconds();
        ops += op! {
            println!("The current time is: {}", ref:now);
            println!("The elapsed time is: {}", ref:elapsed);
            ref mut: timer.start();
            mut: int_pieces = pieces!(
                Linear::constant(0.0),
                (5.seconds(), Linear::new(1.seconds(), 0.0, 1.0)),
                (10.seconds(), Linear::constant(5.0))
            );
        };
        ops.wait(6.seconds());
        ops += op! {
            println!("Timer says {:?}", ref:timer);
            mut: int_pieces = pieces!(Linear::constant(0.0));
        };
        Ok(duration)
    }
}
