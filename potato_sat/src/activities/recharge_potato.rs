use crate::{battery, mode};
use peregrine::Duration;
use peregrine::impl_activity;
use peregrine::reexports::hifitime::TimeUnits;
use serde::{Deserialize, Serialize};

#[derive(Hash, Serialize, Deserialize)]
pub struct RechargePotato {
    pub amount: u32,
}

impl_activity! { for RechargePotato
    let end = start + 1.hours();
    @(end) {
        ref mut: battery += 4.0;
        mut: mode = "help".to_string();
    }
    Duration::ZERO
}
