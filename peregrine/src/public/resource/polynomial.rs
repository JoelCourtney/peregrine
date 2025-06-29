use crate as peregrine;
use crate::MaybeHash;
use crate::Time;
use crate::public::resource::Data;
use hifitime::{Duration, TimeUnits};
use num::Zero;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul};

pub type Linear<Y = f64> = Polynomial<1, Y>;
pub type Quadratic<Y = f64> = Polynomial<2, Y>;
pub type Cubic<Y = f64> = Polynomial<3, Y>;
pub type Quartic<Y = f64> = Polynomial<4, Y>;
pub type Quintic<Y = f64> = Polynomial<5, Y>;

#[derive(Copy, Clone, Serialize, Deserialize, Debug, MaybeHash)]
pub struct Polynomial<const DEGREE: usize, Y: MaybeHash> {
    pub value: Y,
    #[serde(with = "serde_arrays")]
    pub higher_coefficients: [Y; DEGREE],
    pub basis: Duration,
}

impl<
    const DEGREE: usize,
    Y: 'static
        + MaybeHash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + Copy
        + Mul<f64, Output = Y>
        + Add<Output = Y>
        + Zero,
> Data<'_> for Polynomial<DEGREE, Y>
{
    type Read = (Time, Self);
    type Sample = Self;

    fn to_read(&self, written: Time) -> Self::Read {
        (written, *self)
    }

    fn from_read((written, mut this): (Time, Self), now: Time) -> Self {
        let elapsed = now - written;
        let measure = elapsed.to_seconds() / this.basis.to_seconds();

        let mut acc = this.higher_coefficients[DEGREE - 1];
        for i in (0..DEGREE - 1).rev() {
            let old = this.higher_coefficients[i];
            let diff = acc * measure;
            this.higher_coefficients[i] = this.higher_coefficients[i] + diff;
            acc = diff + old;
        }
        this.value = this.value + acc * measure;
        this
    }

    fn sample(read: Self::Read, now: Time) -> Self::Sample {
        Self::from_read(read, now)
    }
}

impl<const DEGREE: usize, Y: Default + Copy + MaybeHash> Default for Polynomial<DEGREE, Y> {
    fn default() -> Self {
        Self {
            value: Default::default(),
            higher_coefficients: [Default::default(); DEGREE],
            basis: 1.seconds(),
        }
    }
}

macro_rules! impl_constructors {
    ($($n:literal => $($etc:ident)*;)*) => {
        $(
            impl<Y: Copy + MaybeHash> Polynomial<$n, Y> {
                pub fn new(basis: Duration, a: Y, $($etc: Y,)*) -> Self {
                    Self {
                        value: a,
                        higher_coefficients: [$($etc,)*],
                        basis
                    }
                }
            }
        )*
    };
}

impl_constructors![
    0 => ;
    1 => b;
    2 => b c;
    3 => b c d;
    4 => b c d e;
    5 => b c d e f;
];

impl<const DEGREE: usize, Y: Copy + Zero + MaybeHash> Polynomial<DEGREE, Y> {
    pub fn constant(a: Y) -> Self {
        Self {
            value: a,
            higher_coefficients: [Y::zero(); DEGREE],
            basis: 1.seconds(),
        }
    }
}

impl<const DEGREE: usize, Y: Copy + MaybeHash> Polynomial<DEGREE, Y> {
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
