use crate::int_pieces;
use peregrine::impl_activity;
use peregrine::pieces;
use peregrine::reexports::hifitime::TimeUnits;
use peregrine::resource::polynomial::Linear;
use serde::{Deserialize, Serialize};

#[derive(Hash, Serialize, Deserialize)]
pub struct RechargePotato {
    pub amount: u32,
}

impl_activity! { for RechargePotato
    let duration = 100.seconds();
    @(start) {
        mut: int_pieces = pieces!(
            Linear::constant(0.0),
            @(5.seconds()) Linear::new(1.seconds(), 0.0, 1.0),
            @(10.seconds()) Linear::constant(5.0)
        );
    }
    duration
}
