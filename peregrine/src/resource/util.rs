use crate::Time;
use crate::macro_prelude::Dynamic;
use hifitime::{Duration, TimeUnits};
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul};

pub type Linear<Y = f64> = Polynomial<1, Y>;
pub type Quadratic<Y = f64> = Polynomial<2, Y>;

#[derive(Copy, Clone, Serialize, Deserialize, Debug)]
pub struct Polynomial<const DEGREE: usize, Y> {
    pub value: Y,
    #[serde(with = "serde_arrays")]
    pub higher_coefficients: [Y; DEGREE],
    pub basis: Duration,
}

impl<const DEGREE: usize, Y: Copy + Mul<f64, Output = Y> + Add<Output = Y> + Zero> Dynamic
    for Polynomial<DEGREE, Y>
{
    type Sample = Self;

    unsafe fn sample(&self, written: Time, now: Time) -> Self::Sample {
        let mut copy = *self;
        copy.evolve(now - written);
        copy
    }

    fn evolve(&mut self, elapsed: Duration) {
        let measure = elapsed.to_seconds() / self.basis.to_seconds();

        let mut acc = self.higher_coefficients[DEGREE - 1];
        for i in (0..DEGREE - 1).rev() {
            let old = self.higher_coefficients[i];
            let diff = acc * measure;
            self.higher_coefficients[i] = self.higher_coefficients[i] + diff;
            acc = diff + old;
        }
        self.value = self.value + acc * measure;
    }
}

impl<const DEGREE: usize, Y: Default + Copy> Default for Polynomial<DEGREE, Y> {
    fn default() -> Self {
        Self {
            value: Default::default(),
            higher_coefficients: [Default::default(); DEGREE],
            basis: 1.seconds(),
        }
    }
}

impl<const DEGREE: usize, Y: Copy> Polynomial<DEGREE, Y> {
    pub fn slope(&self) -> Y {
        self.higher_coefficients[0]
    }

    pub fn acceleration(&self) -> Y {
        self.higher_coefficients[1]
    }

    pub fn jerk(&self) -> Y {
        self.higher_coefficients[2]
    }

    pub fn slope_mut(&mut self) -> &mut Y {
        &mut self.higher_coefficients[0]
    }

    pub fn acceleration_mut(&mut self) -> &mut Y {
        &mut self.higher_coefficients[1]
    }

    pub fn jerk_mut(&mut self) -> &mut Y {
        &mut self.higher_coefficients[2]
    }
}
