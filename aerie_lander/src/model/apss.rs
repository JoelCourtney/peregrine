use std::hash::Hash;

use peregrine::{Data, MaybeHash, model, polynomial::Linear};
use serde::{Deserialize, Serialize};

pub const _LIMIT_RESOLUTION: f64 = 0.0001;

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Component {
    TwinsPy,
    TwinsMy,
    P,
    Ifg,
    ApssBusV,
}

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ComponentRate {
    pub default_rate: f64,
    pub both_booms_on_rate: f64,
}

#[derive(Data, MaybeHash, Debug, Copy, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ComponentModel {
    pub state: bool,
    pub in_rate: ComponentRate,
    pub out_rate: ComponentRate,
}

model! {
    pub Apss {
        pub pae_powered_on: bool = false,
        pub twins_py: ComponentModel,
        pub twins_my: ComponentModel,
        pub p: ComponentModel,
        pub ifg: ComponentModel,
        pub apss_bus_v: ComponentModel,
        pub internal_volume: Linear,
        pub volume_to_send_to_vc: Linear,
        pub continuous_data_sent_in: f64 = 0.0,
        pub transfer_rate: f64 = 751.68 / 3600.0,
    }
}
