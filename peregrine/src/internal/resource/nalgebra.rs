use std::hash::Hasher;

use nalgebra::{
    ArrayStorage, Const, Dim, Matrix, Quaternion, RawStorage, Rotation, Scalar, Unit, VecStorage,
    ViewStorage,
};
use num::Zero;
use serde::{Serialize, de::DeserializeOwned};

use crate::{Data, MaybeHash, Time};

impl<T, const R: usize, const C: usize> MaybeHash
    for Matrix<T, Const<R>, Const<C>, ArrayStorage<T, R, C>>
where
    T: Scalar + MaybeHash,
{
    fn is_hashable(&self) -> bool {
        self.iter().all(|x| x.is_hashable())
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.iter().for_each(|x| x.hash_unchecked(state));
    }
}

impl<T, R, C> MaybeHash for Matrix<T, R, C, VecStorage<T, R, C>>
where
    T: Scalar + MaybeHash,
    R: Dim,
    C: Dim,
    VecStorage<T, R, C>: RawStorage<T, R, C>,
{
    fn is_hashable(&self) -> bool {
        self.data.as_slice().iter().all(|x| x.is_hashable())
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        use std::hash::Hash;
        self.nrows().hash(state);
        self.ncols().hash(state);
        self.data
            .as_slice()
            .iter()
            .for_each(|x| x.hash_unchecked(state));
    }
}

impl<'h, T, R, C> MaybeHash for Matrix<T, R, C, ViewStorage<'h, T, R, C, Const<1>, C>>
where
    T: Scalar + MaybeHash,
    R: Dim,
    C: Dim,
{
    fn is_hashable(&self) -> bool {
        self.iter().all(|x| x.is_hashable())
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.iter().for_each(|x| x.hash_unchecked(state));
    }
}

impl<'h, T, const R: usize, const C: usize> Data<'h>
    for Matrix<T, Const<R>, Const<C>, ArrayStorage<T, R, C>>
where
    ArrayStorage<T, R, C>: DeserializeOwned + Serialize,
    T: Zero + Sync + MaybeHash + Scalar + Send + Copy,
    Self: Serialize + DeserializeOwned,
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

impl<'h, T, R, C> Data<'h> for Matrix<T, R, C, VecStorage<T, R, C>>
where
    R: Dim,
    C: Dim,
    VecStorage<T, R, C>: RawStorage<T, R, C>,
    T: Zero + Sync + MaybeHash + Scalar + Send + Copy,
    Self: Serialize + DeserializeOwned,
{
    type Read = (usize, usize, &'h [T]);
    type Sample = Matrix<T, R, C, ViewStorage<'h, T, R, C, Const<1>, C>>;

    fn to_read(&self, _: Time) -> Self::Read {
        let slice = self.data.as_slice();
        let ptr = slice.as_ptr();
        (self.nrows(), self.ncols(), unsafe {
            std::slice::from_raw_parts(ptr, slice.len())
        })
    }

    fn from_read(read: Self::Read, _: Time) -> Self {
        let mut data = Vec::with_capacity(read.0 * read.1);
        data.extend_from_slice(read.2);
        Matrix::from_data(VecStorage::new(
            R::from_usize(read.0),
            C::from_usize(read.1),
            data,
        ))
    }

    fn sample(read: Self::Read, _: Time) -> Self::Sample {
        let storage = unsafe {
            ViewStorage::from_raw_parts(
                read.2.as_ptr(),
                (R::from_usize(read.0), C::from_usize(read.1)),
                (Const::<1>, C::from_usize(read.1)),
            )
        };
        Matrix::from_data(storage)
    }
}

impl<T> MaybeHash for Quaternion<T>
where
    T: MaybeHash,
    Self: std::fmt::Debug,
{
    fn is_hashable(&self) -> bool {
        self.coords[0].is_hashable()
            && self.coords[1].is_hashable()
            && self.coords[2].is_hashable()
            && self.coords[3].is_hashable()
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.coords.iter().for_each(|x| x.hash_unchecked(state));
    }
}

impl<'h, T> Data<'h> for Quaternion<T>
where
    T: Scalar + Data<'h> + Copy + Serialize + DeserializeOwned,
    Self: Serialize + DeserializeOwned,
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

impl<T, const D: usize> MaybeHash for Rotation<T, D>
where
    T: MaybeHash + Scalar,
{
    fn is_hashable(&self) -> bool {
        self.matrix().is_hashable()
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.matrix().hash_unchecked(state);
    }
}

impl<'h, T, const D: usize> Data<'h> for Rotation<T, D>
where
    T: Send + Sync + Scalar + Copy + Serialize + DeserializeOwned + MaybeHash,
    Self: Serialize + DeserializeOwned,
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

impl<T> MaybeHash for Unit<T>
where
    T: MaybeHash,
{
    fn is_hashable(&self) -> bool {
        self.as_ref().is_hashable()
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash_unchecked(state);
    }
}

impl<'h, T> Data<'h> for Unit<T>
where
    T: Data<'h>,
    Self: Serialize + DeserializeOwned,
{
    type Read = T::Read;
    type Sample = Unit<T::Sample>;

    fn to_read(&self, written: Time) -> Self::Read {
        self.as_ref().to_read(written)
    }

    fn from_read(read: Self::Read, written: Time) -> Self {
        Unit::new_unchecked(T::from_read(read, written))
    }

    fn sample(read: Self::Read, now: Time) -> Self::Sample {
        Unit::new_unchecked(T::sample(read, now))
    }
}
