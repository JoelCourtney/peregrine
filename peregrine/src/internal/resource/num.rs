use std::hash::Hasher;

use num::{BigInt, BigUint, Complex, Integer, rational::Ratio};

use crate::{Data, MaybeHash, Time, impl_maybe_hash_for_hashable};

impl<T: MaybeHash> MaybeHash for Complex<T> {
    fn is_hashable(&self) -> bool {
        self.re.is_hashable() && self.im.is_hashable()
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.re.hash_unchecked(state);
        self.im.hash_unchecked(state);
    }
}

impl<'h, T: Data<'h>> Data<'h> for Complex<T> {
    type Read = (T::Read, T::Read);
    type Sample = Complex<T::Sample>;

    fn to_read(&self, written: Time) -> Self::Read {
        (self.re.to_read(written), self.im.to_read(written))
    }

    fn from_read(read: Self::Read, now: Time) -> Self {
        Self {
            re: T::from_read(read.0, now),
            im: T::from_read(read.1, now),
        }
    }

    fn sample(read: Self::Read, now: Time) -> Self::Sample {
        Complex {
            re: T::sample(read.0, now),
            im: T::sample(read.1, now),
        }
    }
}

impl<T: MaybeHash> MaybeHash for Ratio<T> {
    fn is_hashable(&self) -> bool {
        self.numer().is_hashable() && self.denom().is_hashable()
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.numer().hash_unchecked(state);
        self.denom().hash_unchecked(state);
    }
}

impl<'h, T: Data<'h> + Integer> Data<'h> for Ratio<T> {
    type Read = (T::Read, T::Read);
    type Sample = Ratio<T::Sample>;

    fn to_read(&self, written: Time) -> Self::Read {
        (self.numer().to_read(written), self.denom().to_read(written))
    }

    fn from_read(read: Self::Read, now: Time) -> Self {
        Ratio::new(T::from_read(read.0, now), T::from_read(read.1, now))
    }

    fn sample(read: Self::Read, now: Time) -> Self::Sample {
        Ratio::new_raw(T::sample(read.0, now), T::sample(read.1, now))
    }
}

impl_maybe_hash_for_hashable![BigInt, BigUint];

impl<'h> Data<'h> for Box<BigInt> {
    type Read = &'h BigInt;
    type Sample = &'h BigInt;

    fn to_read(&self, _: Time) -> Self::Read {
        let ptr = &**self as *const BigInt;
        unsafe { &*ptr }
    }

    fn from_read(read: Self::Read, _: Time) -> Self {
        Box::new(read.clone())
    }

    fn sample(read: Self::Read, _: Time) -> Self::Sample {
        read
    }
}

impl<'h> Data<'h> for Box<BigUint> {
    type Read = &'h BigUint;
    type Sample = &'h BigUint;

    fn to_read(&self, _: Time) -> Self::Read {
        let ptr = &**self as *const BigUint;
        unsafe { &*ptr }
    }

    fn from_read(read: Self::Read, _: Time) -> Self {
        Box::new(read.clone())
    }

    fn sample(read: Self::Read, _: Time) -> Self::Sample {
        read
    }
}
