use bigdecimal::BigDecimal;

use crate::{Data, MaybeHash, Time, impl_maybe_hash_for_hashable};

impl_maybe_hash_for_hashable![BigDecimal];

impl<'h> Data<'h> for Box<BigDecimal> {
    type Read = &'h BigDecimal;
    type Sample = &'h BigDecimal;

    fn to_read(&self, _: Time) -> Self::Read {
        let ptr = &**self as *const BigDecimal;
        unsafe { &*ptr }
    }

    fn from_read(read: Self::Read, _: Time) -> Self {
        Box::new(read.clone())
    }

    fn sample(read: Self::Read, _: Time) -> Self::Sample {
        read
    }
}
