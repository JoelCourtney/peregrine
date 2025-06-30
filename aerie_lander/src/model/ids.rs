use std::hash::Hash;

use peregrine::{Data, MaybeHash, model};
use serde::{Deserialize, Serialize};

#[derive(Data, MaybeHash, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IdaMode {
    Idle,
    Moving,
    Grappling,
}

model! {
    pub Ids {
        pub ida_mode: IdaMode = IdaMode::Idle,
        pub ida_survival_heaters_nominal: bool = true,
    }
}

impl Ids {
    pub fn _compute_size(comp_quality: i32) -> f64 {
        let a = 30_226.959_701_602_77;
        let b = -0.066_587_436_103_361_51;
        let c = -1_459.444_507_542_347;
        let d = 0.002_534_592_140_626_373;
        let e = 80.749_713_220_121_04;
        let f = -4.635_963_037_609_322e-5;
        let g = -1.351_910_590_950_06;
        let h = 3.855_556_487_825_687e-7;
        let i = 0.006_743_543_630_872_695;
        let j = -1.187_301_787_888_751e-9;
        let k = 0;
        let l = 1;
        let m = 1200.0;
        let n = 1648.0;
        let o = 0;
        let x = comp_quality;
        let x2 = comp_quality.pow(2);
        let x3 = comp_quality.pow(3);
        let x4 = comp_quality.pow(4);
        let x5 = comp_quality.pow(5);

        let image_size = if comp_quality == 0 {
            (1024 * 1024 * 16) as f64
        } else {
            (((a + c * x as f64 + e * x2 as f64 + g * x3 as f64 + i * x4 as f64)
                / (1.0
                    + b * x as f64
                    + d * x2 as f64
                    + f * x3 as f64
                    + h * x4 as f64
                    + j * x5 as f64))
                * (k as f64 + (l as f64 * 1024.0 * 1024.0) / (m * n))
                + o as f64)
                * 8.0
        };
        image_size / 1.0e6
    }
}
