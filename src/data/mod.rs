pub mod evolving;
pub mod polynomial;

use std::{
    ffi::{CStr, CString},
    time::{Duration, Instant},
};

use hifitime::Epoch;

use crate::{Callback, Ctx, Upstream, cache::Cached};

/// A marker trait for types that can be used as the output of a node.
///
/// Auto-implemented for all types that satisfy the required bounds.
/// You don't need to implement this trait manually.
pub trait Data: PartialEq + Clone + Send + Sync + 'static {}
impl<T> Data for T where T: PartialEq + Clone + Send + Sync + 'static {}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Source<T>(pub T);

impl<T: Data> Upstream for Source<T> {
    type Output = T;

    #[inline]
    fn request<'s>(&self, ctx: Ctx<'_, '_, 's>, callback: Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        callback.call(Cached::Constant(self.0.clone()), ctx);
    }
}

macro_rules! impl_upstream_for_data {
    ($($ty:ty),*) => {
        $(
            // impl Data for $ty {}
            impl Upstream for $ty {
                type Output = Self;

                #[inline(always)]
                fn request<'s>(&self, ctx: Ctx<'_, '_, 's>, callback: Callback<'s, Self::Output>)
                where
                    Self: 's,
                {
                    callback.call(Cached::Constant(self.clone()), ctx);
                }
            }
        )*
    };
}

impl_upstream_for_data! {
    (), bool, i8, i16, i32, i64, i128, isize,
    u8, u16, u32, u64, u128, usize,
    f32, f64,
    String, char, &'static str,
    CString, &'static CStr,
    Duration, Instant,
    Epoch, hifitime::Duration
}

macro_rules! impl_upstream_for_tuple {
    ($($ty:ident),*) => {
            impl<$($ty: Data),*> Upstream for ($($ty,)*) {
                type Output = ($($ty,)*);

                #[inline(always)]
                fn request<'s>(&self, ctx: Ctx<'_, '_, 's>, callback: Callback<'s, Self::Output>)
                where
                    Self: 's,
                {
                    callback.call(Cached::Constant(self.clone()), ctx);
                }
            }
    };
}

impl_upstream_for_tuple!(T1);
impl_upstream_for_tuple!(T1, T2);
impl_upstream_for_tuple!(T1, T2, T3);
impl_upstream_for_tuple!(T1, T2, T3, T4);
impl_upstream_for_tuple!(T1, T2, T3, T4, T5);
impl_upstream_for_tuple!(T1, T2, T3, T4, T5, T6);
impl_upstream_for_tuple!(T1, T2, T3, T4, T5, T6, T7);
impl_upstream_for_tuple!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_upstream_for_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_upstream_for_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_upstream_for_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_upstream_for_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

impl<T: Data> Upstream for Vec<T> {
    type Output = Self;

    fn request<'s>(&self, ctx: Ctx<'_, '_, 's>, callback: Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        callback.call(Cached::Constant(self.clone()), ctx);
    }
}
impl<T: Data, const N: usize> Upstream for [T; N] {
    type Output = Self;

    fn request<'s>(&self, ctx: Ctx<'_, '_, 's>, callback: Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        callback.call(Cached::Constant(self.clone()), ctx);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate as peregrine;
    use crate::{graph::variable::Var, run};
    use peregrine_macros::AutoSource;

    #[derive(PartialEq, Debug)]
    struct NonUpstream;

    #[test]
    fn source() {
        let v = Var::new(Source(Arc::new(NonUpstream)));

        assert_eq!(run(v), Arc::new(NonUpstream));
    }

    #[derive(AutoSource, Copy, Clone, PartialEq, Debug)]
    struct MakeAutoSourcePlease;

    #[test]
    fn auto_source() {
        let v = Var::new(MakeAutoSourcePlease);

        assert_eq!(run(v), MakeAutoSourcePlease);
    }
}
