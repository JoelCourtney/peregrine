use crate::Time;
use crate::resource::{Data, MaybeHash};
use hifitime::{Duration, TimeUnits};
use num_traits::Zero;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::hash::Hasher;
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

    fn sample(read: &Self::Read, now: Time) -> Self::Sample {
        Self::from_read(*read, now)
    }
}

impl<const DEGREE: usize, Y: MaybeHash> MaybeHash for Polynomial<DEGREE, Y> {
    fn is_hashable(&self) -> bool {
        self.basis.is_hashable() && self.higher_coefficients.iter().all(|c| c.is_hashable())
    }
    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.basis.hash_unchecked(state);
        self.higher_coefficients
            .iter()
            .for_each(|c| c.hash_unchecked(state));
        self.value.hash_unchecked(state);
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

// #[derive(Clone, Serialize, Deserialize, Debug)]
// pub struct PiecewiseConstant<T> {
//     default: Box<T>,
//     pieces: SmallVec<(Duration, T), 2>
// }
//
// pub struct PiecewiseConstantBorrow<'h, T> {
//     default: &'h T,
//     pieces: &'h [(Duration, T)],
// }
//
// impl<'h, T: 'h + Clone> From<PiecewiseConstantBorrow<'h, T>> for PiecewiseConstant<T> {
//     fn from(value: PiecewiseConstantBorrow<'h, T>) -> Self {
//         Self {
//             default: value.default.clone(),
//             pieces: SmallVec::from(value.pieces),
//         }
//     }
// }
//
// impl<T: 'static + Clone> CustomStorage for PiecewiseConstant<T> {
//     type Read<'h> = PiecewiseConstantBorrow<'h, T> where Self: 'h;
//
//     unsafe fn to_read(&self) -> Self::Read<'_> {
//         PiecewiseConstantBorrow {
//             default: &*self.default,
//             pieces: &self.pieces[..],
//         }
//     }
// }
//
// impl<'h, T: Copy> Dynamic for PiecewiseConstantBorrow<'h, T> {
//     type Sample = &'h T;
//
//     unsafe fn sample(&self, written: Time, now: Time) -> Self::Sample {
//         let elapsed = now - written;
//         let mut index = 0;
//         while index < self.pieces.len() && self.pieces[index].0 <= elapsed {
//             index += 1;
//         }
//         if index == 0 {
//             self.default
//         } else {
//             self.pieces[index - 1].1
//         }
//     }
//
//     fn evolve(&mut self, elapsed: Duration) {
//         self.pieces.retain_mut(|(t,v)| {
//             *t -= elapsed;
//             if *t <= Duration::ZERO {
//                 self.default = *v;
//                 false
//             } else {
//                 true
//             }
//         })
//     }
// }
//
// #[macro_export]
// macro_rules! pieces {
//     ($default:expr; $(@($dur:expr) $value:expr)*) => {
//         $crate::resource::util:Piecewise {
//             default: $default,
//             pieces: $crate::reexports::smallvec::#SmallVec::from_slice(&[$(($dur, $value)),*])
//         }
//     };
// }
