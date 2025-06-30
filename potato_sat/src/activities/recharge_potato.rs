use crate::int_pieces;
use crate::timer;
use peregrine::anyhow::Result;
use peregrine::hifitime::TimeUnits;
use peregrine::public::activity::{Activity, Ops, OpsReceiver};
use peregrine::public::resource::builtins::{elapsed, now};
use peregrine::public::resource::polynomial::Linear;
use peregrine::{Duration, op, pieces};
use serde::{Deserialize, Serialize};

#[derive(Hash, Serialize, Deserialize)]
pub struct RechargePotato {
    pub amount: u32,
}

#[typetag::serde]
impl Activity for RechargePotato {
    fn run(&self, mut ops: Ops) -> Result<Duration> {
        let duration = 100.seconds();
        ops += op! {
            println!("The current time is: {}", r:now);
            println!("The elapsed time is: {}", r:elapsed);
            m:timer.start();
            w:int_pieces = pieces!(
                Linear::constant(0.0),
                (5.seconds(), Linear::new(1.seconds(), 0.0, 1.0)),
                (10.seconds(), Linear::constant(5.0))
            );
        };
        ops.wait(6.seconds());
        ops += op! {
            println!("Timer says {:?}", r:timer);
            w:int_pieces = pieces!(Linear::constant(0.0));
        };
        Ok(duration)
    }
}
