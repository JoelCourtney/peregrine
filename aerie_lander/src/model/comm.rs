use std::hash::Hash;

use peregrine::{Data, Linear, MaybeHash, model};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Orbiter {
    _Ody,
    _Mro,
    _Tgo,
    _Mvn,
    _Mex,
}

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum XBandAntenna {
    EastMga,
    WestMga,
}

model! {
    pub Comm {
        pub data_sent: Linear;
        pub active_xband_antenna: XBandAntenna = XBandAntenna::EastMga;
        pub alternate_uhf_block_in_use_ody: bool = false;
        pub alternate_uhf_block_in_use_mro: bool = false;
        pub alternate_uhf_block_in_use_tgo: bool = false;
        pub alternate_uhf_block_in_use_mvn: bool = false;
        pub alternate_uhf_block_in_use_mex: bool = false;
    }
}
