use std::hash::Hash;

use peregrine::hifitime::TimeUnits;
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
        pub power_state: PowerState = PowerState::Off;
        pub heat_probe_internal_data: Linear;
        pub sci_data_sent_in_activity: f64 = 0.0;
        pub ssa_state: SsaState = SsaState::Off;
        pub rad_state: RadState = RadState::Off;
        pub heat_probe_mon_temp_duration_table: Duration = 5.0.minutes();
        pub heat_probe_mon_wait_duration_table: Duration = 55.0.minutes();
        pub heat_probe_singlepen_cool_duration_table: Duration = 3.0.hours();
        pub heat_probe_singlepen_tema_duration_table: Duration = 1.0.hours();
        pub heat_probe_hammer_timeout_table: Duration = 4.0.hours();
        pub heat_probe_co_temp_duration_table: Duration = 10.0.minutes();
        pub heat_probe_co_tema_duration_table: Duration = 12.0.minutes();
        pub heat_probe_co_statil_tlm_duration_table: Duration = 14.0.minutes();
        pub rad_heatup_duration_table: Duration = 15.0.minutes();
        pub rad_meas_duration_table: Duration = 20.0.minutes();
        pub rad_hourly_wait_duration_table: Duration = 56.0.minutes() + 4.0.seconds();
        pub rad_std_wait_duration_short_table: Duration = 2.0.hours() + 4.0.minutes() + 58.0.seconds();
        pub rad_std_wait_duration_long_table: Duration = 8.0.hours() + 14.0.minutes() + 52.0.seconds();
        pub rad_singlemeas_duration_table: Duration = 15.0.minutes();
        pub rad_cal_meas_duration_table: Duration = 5.0.minutes();
        pub heat_probe_mon_temp_duration_current: Duration = 5.0.minutes();
        pub heat_probe_mon_wait_duration_current: Duration = 55.0.minutes();
        pub heat_probe_singlepen_cool_duration_current: Duration = 3.0.hours();
        pub heat_probe_singlepen_tema_duration_current: Duration = 1.0.hours();
        pub heat_probe_hammer_timeout_current: Duration = 4.0.hours();
        pub heat_probe_co_temp_duration_current: Duration = 10.0.minutes();
        pub heat_probe_co_tema_duration_current: Duration = 12.0.minutes();
        pub heat_probe_co_statil_tlm_duration_current: Duration = 14.0.minutes();
        pub rad_heatup_duration_current: Duration = 15.0.minutes();
        pub rad_meas_duration_current: Duration = 20.0.minutes();
        pub rad_hourly_wait_duration_current: Duration = 56.0.minutes() + 4.0.seconds();
        pub rad_std_wait_duration_short_current: Duration = 2.0.hours() + 4.0.minutes() + 58.0.seconds();
        pub rad_std_wait_duration_long_current: Duration = 8.0.hours() + 14.0.minutes() + 52.0.seconds();
        pub rad_singlemeas_duration_current: Duration = 15.0.minutes();
        pub rad_cal_meas_duration_current: Duration = 5.0.minutes();
    }
}
