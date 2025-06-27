use crate::Time;
use crate::public::resource::{Data, MaybeHash};
use duplicate::duplicate_item;
use hifitime::Duration;
use std::cell::OnceCell;
use std::hash::{Hash, Hasher};
use std::ops::Deref;

macro_rules! impl_copy_static_data {
    ($($t:ty),*) => {
        $(
            impl<'h> $crate::public::resource::Data<'h> for $t where Self: Copy {
                type Read = $t;
                type Sample = $t;

                fn to_read(&self, _written: Time) -> Self::Read {
                    *self
                }
                fn from_read(read: Self::Read, _now: Time) -> Self {
                    read
                }
                fn sample(read: &Self::Read, _now: Time) -> Self::Sample {
                    *read
                }
            }
        )*
    };
}

impl_copy_static_data![
    u8,
    u32,
    u64,
    u128,
    i8,
    i32,
    i64,
    i128,
    f32,
    f64,
    bool,
    char,
    Duration,
    Time,
    ()
];

macro_rules! impl_deref_static_data {
    ($($t:ty),*) => {
        $(
            impl<'h> $crate::public::resource::Data<'h> for $t where Self: 'h {
                type Read = &'h <$t as Deref>::Target;
                type Sample = &'h <$t as Deref>::Target;

                fn to_read(&self, _written: Time) -> Self::Read {
                    let ptr = &**self as *const <$t as Deref>::Target;
                    unsafe { &*ptr }
                }
                fn from_read(read: Self::Read, _now: Time) -> Self {
                    read.into()
                }
                fn sample(read: &Self::Read, _now: Time) -> Self::Sample {
                    *read
                }
            }
        )*
    };
}

impl_deref_static_data![String];

impl<'h, T: Data<'h>> Data<'h> for Vec<T> {
    type Read = (Time, &'h [T]);
    type Sample = SliceSampler<'h, T>;

    fn to_read(&self, written: Time) -> Self::Read {
        let ptr = self.as_slice().as_ptr();
        (written, unsafe {
            std::slice::from_raw_parts(ptr, self.len())
        })
    }

    fn from_read(read: Self::Read, now: Time) -> Self {
        read.1
            .iter()
            .map(|v| T::from_read(v.to_read(read.0), now))
            .collect()
    }

    fn sample(read: &Self::Read, now: Time) -> Self::Sample {
        SliceSampler {
            data: read.1,
            written: read.0,
            now,
        }
    }
}

pub struct SliceSampler<'h, T> {
    data: &'h [T],
    written: Time,
    now: Time,
}

impl<'h, T: Data<'h>> Clone for SliceSampler<'h, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'h, T: Data<'h>> Copy for SliceSampler<'h, T> {}

impl<'h, T: Data<'h>> SliceSampler<'h, T> {
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<T::Sample> {
        if index >= self.len() {
            None
        } else {
            Some(T::sample(&self.data[index].to_read(self.written), self.now))
        }
    }

    pub fn first(&self) -> Option<T::Sample> {
        self.get(0)
    }
    pub fn last(&self) -> Option<T::Sample> {
        self.get(self.len().saturating_sub(1))
    }
}

#[duplicate_item(
    ty iter;
    [Vec<T>] [self];
    [SliceSampler<'h, T>] [self.data];
)]
impl<'h, T: Data<'h>> MaybeHash for ty {
    fn is_hashable(&self) -> bool {
        self.is_empty() || self.first().unwrap().is_hashable()
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        for t in iter {
            t.hash_unchecked(state);
        }
    }
}

impl<'h, T: Data<'h>> Data<'h> for Box<T> {
    type Read = (Time, &'h T);
    type Sample = RefSampler<'h, T>;

    fn to_read(&self, written: Time) -> Self::Read {
        let ptr = &**self as *const T;
        let read = unsafe { &*ptr };
        (written, read)
    }

    fn from_read(read: Self::Read, now: Time) -> Self {
        Box::new(T::from_read(read.1.to_read(read.0), now))
    }

    fn sample(read: &Self::Read, now: Time) -> Self::Sample {
        RefSampler {
            data: read.1,
            sample: OnceCell::new(),
            written: read.0,
            now,
        }
    }
}

pub struct RefSampler<'h, T: Data<'h>> {
    data: &'h T,
    sample: OnceCell<T::Sample>,
    written: Time,
    now: Time,
}

impl<'h, T: Data<'h>> Clone for RefSampler<'h, T> {
    fn clone(&self) -> Self {
        Self {
            data: self.data,
            sample: OnceCell::new(),
            written: self.written,
            now: self.now,
        }
    }
}

impl<'h, T: Data<'h>> Deref for RefSampler<'h, T> {
    type Target = T::Sample;

    fn deref(&self) -> &Self::Target {
        self.sample
            .get_or_init(|| T::sample(&self.data.to_read(self.written), self.now))
    }
}

#[duplicate_item(
    ty;
    [Box<T>];
    [RefSampler<'h, T>];
)]
impl<'h, T: Data<'h>> MaybeHash for ty {
    fn is_hashable(&self) -> bool {
        self.deref().is_hashable()
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        self.deref().hash_unchecked(state);
    }
}

#[duplicate_item(
    ty;
    [u8];
    [u16];
    [u32];
    [u64];
    [u128];
    [i8];
    [i16];
    [i32];
    [i64];
    [i128];
    [bool];
    [char];
    [Duration];
    [Time];
    [&'_ str];
    [String];
    [()];
)]
impl MaybeHash for ty
where
    Self: Hash,
{
    fn is_hashable(&self) -> bool {
        true
    }
    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        use std::hash::Hash;
        self.hash(state);
    }
}

impl<T: MaybeHash> MaybeHash for Option<T> {
    fn is_hashable(&self) -> bool {
        self.as_ref().map(|t| t.is_hashable()).unwrap_or(true)
    }

    fn hash_unchecked<H: Hasher>(&self, state: &mut H) {
        if let Some(t) = self {
            true.hash(state);
            t.hash_unchecked(state);
        } else {
            false.hash(state);
        }
    }
}
