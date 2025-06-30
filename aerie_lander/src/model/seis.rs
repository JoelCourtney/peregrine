use std::hash::Hash;

use peregrine::{Data, MaybeHash, model, polynomial::Linear};
use serde::{Deserialize, Serialize};

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Gain {
    Low,
    High,
}

impl Gain {
    pub fn _abbrev(&self) -> &'static str {
        match self {
            Gain::Low => "LG",
            Gain::High => "HG",
        }
    }
}

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Device {
    Vbb1,
    Vbb2,
    Vbb3,
    Sp1,
    Sp2,
    Sp3,
    Scit,
}

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeviceType {
    Vel,
    Pos,
    Temp,
    Sp,
    Scit,
}

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Channel {
    Vbb1VelLrLgEn,
    Vbb1VelLrLgSc,
    Vbb1VelLrHgEn,
    Vbb1VelLrHgSc,
    Vbb1VelHrLgEn,
    Vbb1VelHrLgSc,
    Vbb1VelHrHgEn,
    Vbb1VelHrHgSc,
    Vbb1PosLrLgEn,
    Vbb1PosLrLgSc,
    Vbb1PosLrHgEn,
    Vbb1PosLrHgSc,
    Vbb1PosHrLgEn,
    Vbb1PosHrLgSc,
    Vbb1PosHrHgEn,
    Vbb1PosHrHgSc,
    Vbb1TmpLr,
    Vbb1TmpHr,
    Vbb2VelLrLgEn,
    Vbb2VelLrLgSc,
    Vbb2VelLrHgEn,
    Vbb2VelLrHgSc,
    Vbb2VelHrLgEn,
    Vbb2VelHrLgSc,
    Vbb2VelHrHgEn,
    Vbb2VelHrHgSc,
    Vbb2PosLrLgEn,
    Vbb2PosLrLgSc,
    Vbb2PosLrHgEn,
    Vbb2PosLrHgSc,
    Vbb2PosHrLgEn,
    Vbb2PosHrLgSc,
    Vbb2PosHrHgEn,
    Vbb2PosHrHgSc,
    Vbb2TmpLr,
    Vbb2TmpHr,
    Vbb3VelLrLgEn,
    Vbb3VelLrLgSc,
    Vbb3VelLrHgEn,
    Vbb3VelLrHgSc,
    Vbb3VelHrLgEn,
    Vbb3VelHrLgSc,
    Vbb3VelHrHgEn,
    Vbb3VelHrHgSc,
    Vbb3PosLrLgEn,
    Vbb3PosLrLgSc,
    Vbb3PosLrHgEn,
    Vbb3PosLrHgSc,
    Vbb3PosHrLgEn,
    Vbb3PosHrLgSc,
    Vbb3PosHrHgEn,
    Vbb3PosHrHgSc,
    Vbb3TmpLr,
    Vbb3TmpHr,
    Sp1LrLg,
    Sp1LrHg,
    Sp1HrLg,
    Sp1HrHg,
    Sp2LrLg,
    Sp2LrHg,
    Sp2HrLg,
    Sp2HrHg,
    Sp3LrLg,
    Sp3LrHg,
    Sp3HrLg,
    Sp3HrHg,
    ScitHr,
    ScitLr,
}

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VbbMode {
    Sci,
    Eng,
}

impl VbbMode {
    pub fn _abbrev(&self) -> &'static str {
        match self {
            VbbMode::Sci => "SC",
            VbbMode::Eng => "EN",
        }
    }
}

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ChannelRate {
    pub in_rate: f64,
    pub out_rate: f64,
}

#[derive(Data, MaybeHash, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelOutRateGroup {
    pub out_rate: f64,
    pub channels: Vec<Channel>,
}

#[derive(Data, MaybeHash, Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceTypeMetrics {
    pub sampling_rate: f64,
    pub gain: Gain,
}

model! {
    pub Seis {
        pub powered_on: bool = false,
        pub mde_should_be_on: bool = false,
        pub combined_channel_out_rates: Vec<ChannelOutRateGroup> = Vec::new(),
        pub internal_volume: Linear,
        pub volume_to_send_to_vc: Linear,
        pub continuous_data_sent_in: f64 = 0.0,
        pub transfer_rate: f64 = 1666.66 / 3600.0,
        pub vbb_mode: VbbMode = VbbMode::Sci,
        pub vbb1_on: bool = false,
        pub vbb2_on: bool = false,
        pub vbb3_on: bool = false,
        pub sp1_on: bool = false,
        pub sp2_on: bool = false,
        pub sp3_on: bool = false,
        pub scit_on: bool = false,
        pub channel_vbb1_vel_lr_lg_en: ChannelRate,
        pub channel_vbb1_vel_lr_lg_sc: ChannelRate,
        pub channel_vbb1_vel_lr_hg_en: ChannelRate,
        pub channel_vbb1_vel_lr_hg_sc: ChannelRate,
        pub channel_vbb1_vel_hr_lg_en: ChannelRate,
        pub channel_vbb1_vel_hr_lg_sc: ChannelRate,
        pub channel_vbb1_vel_hr_hg_en: ChannelRate,
        pub channel_vbb1_vel_hr_hg_sc: ChannelRate,
        pub channel_vbb1_pos_lr_lg_en: ChannelRate,
        pub channel_vbb1_pos_lr_lg_sc: ChannelRate,
        pub channel_vbb1_pos_lr_hg_en: ChannelRate,
        pub channel_vbb1_pos_lr_hg_sc: ChannelRate,
        pub channel_vbb1_pos_hr_lg_en: ChannelRate,
        pub channel_vbb1_pos_hr_lg_sc: ChannelRate,
        pub channel_vbb1_pos_hr_hg_en: ChannelRate,
        pub channel_vbb1_pos_hr_hg_sc: ChannelRate,
        pub channel_vbb1_tmp_lr: ChannelRate,
        pub channel_vbb1_tmp_hr: ChannelRate,
        pub channel_vbb2_vel_lr_lg_en: ChannelRate,
        pub channel_vbb2_vel_lr_lg_sc: ChannelRate,
        pub channel_vbb2_vel_lr_hg_en: ChannelRate,
        pub channel_vbb2_vel_lr_hg_sc: ChannelRate,
        pub channel_vbb2_vel_hr_lg_en: ChannelRate,
        pub channel_vbb2_vel_hr_lg_sc: ChannelRate,
        pub channel_vbb2_vel_hr_hg_en: ChannelRate,
        pub channel_vbb2_vel_hr_hg_sc: ChannelRate,
        pub channel_vbb2_pos_lr_lg_en: ChannelRate,
        pub channel_vbb2_pos_lr_lg_sc: ChannelRate,
        pub channel_vbb2_pos_lr_hg_en: ChannelRate,
        pub channel_vbb2_pos_lr_hg_sc: ChannelRate,
        pub channel_vbb2_pos_hr_lg_en: ChannelRate,
        pub channel_vbb2_pos_hr_lg_sc: ChannelRate,
        pub channel_vbb2_pos_hr_hg_en: ChannelRate,
        pub channel_vbb2_pos_hr_hg_sc: ChannelRate,
        pub channel_vbb2_tmp_lr: ChannelRate,
        pub channel_vbb2_tmp_hr: ChannelRate,
        pub channel_vbb3_vel_lr_lg_en: ChannelRate,
        pub channel_vbb3_vel_lr_lg_sc: ChannelRate,
        pub channel_vbb3_vel_lr_hg_en: ChannelRate,
        pub channel_vbb3_vel_lr_hg_sc: ChannelRate,
        pub channel_vbb3_vel_hr_lg_en: ChannelRate,
        pub channel_vbb3_vel_hr_lg_sc: ChannelRate,
        pub channel_vbb3_vel_hr_hg_en: ChannelRate,
        pub channel_vbb3_vel_hr_hg_sc: ChannelRate,
        pub channel_vbb3_pos_lr_lg_en: ChannelRate,
        pub channel_vbb3_pos_lr_lg_sc: ChannelRate,
        pub channel_vbb3_pos_lr_hg_en: ChannelRate,
        pub channel_vbb3_pos_lr_hg_sc: ChannelRate,
        pub channel_vbb3_pos_hr_lg_en: ChannelRate,
        pub channel_vbb3_pos_hr_lg_sc: ChannelRate,
        pub channel_vbb3_pos_hr_hg_en: ChannelRate,
        pub channel_vbb3_pos_hr_hg_sc: ChannelRate,
        pub channel_vbb3_tmp_lr: ChannelRate,
        pub channel_vbb3_tmp_hr: ChannelRate,
        pub channel_sp1_lr_lg: ChannelRate,
        pub channel_sp1_lr_hg: ChannelRate,
        pub channel_sp1_hr_lg: ChannelRate,
        pub channel_sp1_hr_hg: ChannelRate,
        pub channel_sp2_lr_lg: ChannelRate,
        pub channel_sp2_lr_hg: ChannelRate,
        pub channel_sp2_hr_lg: ChannelRate,
        pub channel_sp2_hr_hg: ChannelRate,
        pub channel_sp3_lr_lg: ChannelRate,
        pub channel_sp3_lr_hg: ChannelRate,
        pub channel_sp3_hr_lg: ChannelRate,
        pub channel_sp3_hr_hg: ChannelRate,
        pub channel_scit_hr: ChannelRate,
        pub channel_scit_lr: ChannelRate,
        pub device_type_vel_sampling_rate: f64 = 0.0,
        pub device_type_pos_sampling_rate: f64 = 0.0,
        pub device_type_temp_sampling_rate: f64 = 0.0,
        pub device_type_sp_sampling_rate: f64 = 0.0,
        pub device_type_scit_sampling_rate: f64 = 0.0,
        pub device_type_vel_gain: Gain = Gain::High,
        pub device_type_pos_gain: Gain = Gain::High,
        pub device_type_temp_gain: Gain = Gain::High,
        pub device_type_sp_gain: Gain = Gain::High,
        pub device_type_scit_gain: Gain = Gain::High,
    }
}
