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
        pub current_station: DsnStation = DsnStation::None;
        pub canberra: StationState = StationState {
            allocated: Allocated::NotAllocated,
            visible: Visibility::Hidden,
        };
        pub madrid: StationState = StationState {
            allocated: Allocated::NotAllocated,
            visible: Visibility::Hidden,
        };
        pub goldstone: StationState = StationState {
            allocated: Allocated::NotAllocated,
            visible: Visibility::Hidden,
        };
    }
    react(canberra) set_active_station::<canberra>(DsnStation::Canberra);
    react(madrid) set_active_station::<madrid>(DsnStation::Madrid);
    react(goldstone) set_active_station::<goldstone>(DsnStation::Goldstone);
}

fn set_active_station<STATION: Resource<Data = StationState>>(mut ops: Ops, station: DsnStation) {
    ops += op! {
        if r: STATION.allocated == Allocated::Allocated && r: STATION.visible == Visibility::InView {
            m: current_station = station;
        } else if r: current_station == station {
            m: current_station = DsnStation::None;
        }
    };
}
