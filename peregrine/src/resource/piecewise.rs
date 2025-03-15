use crate::Time;
use crate::macro_prelude::{Data, MaybeHash};
use hifitime::Duration;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::hash::Hasher;
use std::mem::transmute;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Piecewise<T> {
    pub default: Box<T>,
    pub pieces: SmallVec<(Duration, T), 2>,
}

impl<'h, T: Data<'h> + Clone> MaybeHash for Piecewise<T> {
    fn is_hashable(&self) -> bool {
        self.default.is_hashable() && self.pieces.iter().all(|(_, piece)| piece.is_hashable())
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.default.hash_unchecked(state);
        self.pieces
            .iter()
            .for_each(|(_, piece)| piece.hash_unchecked(state));
    }
}

impl<'h, T: Data<'h> + Clone> Data<'h> for Piecewise<T> {
    type Read = (Time, &'h T, &'h [(Duration, T)]);
    type Sample = T::Sample;

    fn to_read(&self, written: Time) -> Self::Read {
        unsafe {
            (
                written,
                transmute::<&T, &T>(&*self.default),
                transmute::<&[(Duration, T)], &[(Duration, T)]>(&self.pieces[..]),
            )
        }
    }

    fn from_read(read: Self::Read, now: Time) -> Self {
        let elapsed = now - read.0;
        let mut index = 0;
        while index < read.2.len() && read.2[index].0 <= elapsed {
            index += 1;
        }
        let (start, default) = if index == 0 {
            (read.0, read.1.clone())
        } else {
            (read.0 + read.2[index - 1].0, read.2[index - 1].1.clone())
        };
        let new_pieces = SmallVec::from_iter(
            read.2[index..]
                .iter()
                .map(|(t, v)| (*t - elapsed, v.clone())),
        );
        Piecewise {
            default: Box::new(T::from_read(default.to_read(start), now)),
            pieces: new_pieces,
        }
    }

    fn sample(read: &Self::Read, now: Time) -> Self::Sample {
        let elapsed = now - read.0;
        let mut index = 0;
        while index < read.2.len() && read.2[index].0 <= elapsed {
            index += 1;
        }
        let (start, selection) = if index == 0 {
            (read.0, read.1.clone())
        } else {
            (read.0 + read.2[index - 1].0, read.2[index - 1].1.clone())
        };
        T::sample(&selection.to_read(start), now)
    }
}

#[macro_export]
macro_rules! pieces {
    ($default:expr) => {
        $crate::resource::piecewise::Piecewise {
            default: Box::new($default),
            pieces: $crate::reexports::smallvec::SmallVec::new()
        }
    };
    ($default:expr, $(($dur:expr, $value:expr)),* $(,)?) => {
        $crate::resource::piecewise::Piecewise {
            default: Box::new($default),
            pieces: $crate::reexports::smallvec::SmallVec::from_slice(&[$(($dur, $value)),*])
        }
    };
}
