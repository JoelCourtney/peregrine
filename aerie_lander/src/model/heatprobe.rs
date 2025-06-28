use std::hash::Hash;

use peregrine::{Data, Duration, MaybeHash, model, polynomial::Linear};
use serde::{Deserialize, Serialize};

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PowerState {
    On,
    Off,
}

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SsaState {
    Off,
    Idle,
    Checkout,
    Single,
    Monitoring,
}

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RadState {
    Off,
    Idle,
    Single,
    Calibration,
    Standard,
    Hourly,
}

model! {
    pub HeatProbe {
        pub power_state: PowerState,
        pub heat_probe_internal_data: Linear,
        pub sci_data_sent_in_activity: f64,
        pub ssa_state: SsaState,
        pub rad_state: RadState,
        pub heat_probe_mon_temp_duration_table: Duration,
        pub heat_probe_mon_wait_duration_table: Duration,
        pub heat_probe_singlepen_cool_duration_table: Duration,
        pub heat_probe_singlepen_tema_duration_table: Duration,
        pub heat_probe_hammer_timeout_table: Duration,
        pub heat_probe_co_temp_duration_table: Duration,
        pub heat_probe_co_tema_duration_table: Duration,
        pub heat_probe_co_statil_tlm_duration_table: Duration,
        pub rad_heatup_duration_table: Duration,
        pub rad_meas_duration_table: Duration,
        pub rad_hourly_wait_duration_table: Duration,
        pub rad_std_wait_duration_short_table: Duration,
        pub rad_std_wait_duration_long_table: Duration,
        pub rad_singlemeas_duration_table: Duration,
        pub rad_cal_meas_duration_table: Duration,
        pub heat_probe_mon_temp_duration_current: Duration,
        pub heat_probe_mon_wait_duration_current: Duration,
        pub heat_probe_singlepen_cool_duration_current: Duration,
        pub heat_probe_singlepen_tema_duration_current: Duration,
        pub heat_probe_hammer_timeout_current: Duration,
        pub heat_probe_co_temp_duration_current: Duration,
        pub heat_probe_co_tema_duration_current: Duration,
        pub heat_probe_co_statil_tlm_duration_current: Duration,
        pub rad_heatup_duration_current: Duration,
        pub rad_meas_duration_current: Duration,
        pub rad_hourly_wait_duration_current: Duration,
        pub rad_std_wait_duration_short_current: Duration,
        pub rad_std_wait_duration_long_current: Duration,
        pub rad_singlemeas_duration_current: Duration,
        pub rad_cal_meas_duration_current: Duration,
    }
}
