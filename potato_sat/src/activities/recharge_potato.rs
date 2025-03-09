use crate::line;
use peregrine::impl_activity;
use peregrine::reexports::hifitime::TimeUnits;
use serde::{Deserialize, Serialize};

#[derive(Hash, Serialize, Deserialize)]
pub struct RechargePotato {
    pub amount: u32,
}

impl_activity! { for RechargePotato
    let duration = 100.seconds();
    @(start) {
        *ref mut: line.acceleration_mut() += 1.0;
    }
    @(start + duration) {
        *ref mut: line.acceleration_mut() -= 1.0;
    }
    duration
}
