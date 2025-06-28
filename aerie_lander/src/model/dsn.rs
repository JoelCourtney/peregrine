use std::hash::Hash;

use peregrine::{Data, MaybeHash, Ops, Resource, model, op};
use serde::{Deserialize, Serialize};

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DsnStation {
    Canberra,
    Madrid,
    Goldstone,
    None,
}

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Visibility {
    InView,
    Hidden,
}

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Allocated {
    Allocated,
    NotAllocated,
}

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StationState {
    pub allocated: Allocated,
    pub visible: Visibility,
}

model! {
    pub Dsn {
        pub current_station: DsnStation,
        pub canberra: StationState,
        pub madrid: StationState,
        pub goldstone: StationState,

        react(canberra) set_active_station::<canberra>(DsnStation::Canberra),
        react(madrid) set_active_station::<madrid>(DsnStation::Madrid),
        react(goldstone) set_active_station::<goldstone>(DsnStation::Goldstone),
   }
}

fn set_active_station<STATION: Resource<Data = StationState>>(mut ops: Ops, station: DsnStation) {
    ops += op! {
        if ref: STATION.allocated == Allocated::Allocated && ref: STATION.visible == Visibility::InView {
            ref mut: current_station = station;
        } else if ref: current_station == station {
            ref mut: current_station = DsnStation::None;
        }
    };
}
