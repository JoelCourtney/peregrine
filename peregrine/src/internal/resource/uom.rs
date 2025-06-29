use std::hash::Hasher;

use num::Num;
use uom::{
    Conversion,
    si::{Dimension, Units},
};

use crate::{Data, MaybeHash, Time};

impl<D, U, V> MaybeHash for uom::si::Quantity<D, U, V>
where
    D: Dimension + ?Sized,
    U: Units<V> + ?Sized,
    V: Num + Conversion<V> + MaybeHash,
{
    fn is_hashable(&self) -> bool {
        self.value.is_hashable()
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.value.hash_unchecked(state);
    }
}

impl<'h, D, U, V> Data<'h> for uom::si::Quantity<D, U, V>
where
    D: Dimension + ?Sized + 'static,
    U: Units<V> + ?Sized + 'static,
    V: Num + Conversion<V> + MaybeHash + Data<'h> + Copy,
{
    type Read = Self;
    type Sample = Self;

    fn to_read(&self, _: Time) -> Self::Read {
        *self
    }

    fn from_read(read: Self::Read, _: Time) -> Self {
        read
    }

    fn sample(read: Self::Read, _: Time) -> Self::Sample {
        read
    }
}
