pub mod util;

use crate::Time;
use crate::history::HistoryAdapter;
use derive_more::{Deref, DerefMut};
use hifitime::Duration;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::cell::OnceCell;
use std::hash::Hasher;
use std::ops::Deref;
use type_map::concurrent::TypeMap;
use type_reg::untagged::TypeReg;

/// Marks a type as a resource label.
///
/// Resources are not part of a model, the model is a selection of existing resources. This allows
/// activities, which are also not part of a model, to be applied to any model that has the relevant
/// resources.
///
/// ## Reading & Writing
///
/// Resources are not represented one data type, but two, one for reading and one for writing.
/// For simple [Copy] resources these two types will be the same, and you won't have to worry about it.
/// For more complex resources they may be different but related types, like [String] and [&str][str].
/// This is for performance reasons, to avoid unnecessary cloning of heap-allocated data.
pub trait Resource<'h>: Sync + ErasedResource<'h> {
    const LABEL: &'static str;

    /// Whether the resource represents a value that can vary even when not actively written to by
    /// an operation. This is used for cache invalidation.
    const DYNAMIC: bool;

    const ID: u64;

    /// The type that is read from history.
    type Read: 'h + Copy + Send + Sync;

    /// The type that is written from operations to history.
    type Write: 'h + Clone + Serialize + DeserializeOwned + Send + Sync;

    /// The type of history container to use to store instances of the `Write` type, currently
    /// either [CopyHistory] or [DerefHistory]. See [Resource] for details.
    type History: 'static + HistoryAdapter<Self::Write, Self::Read> + Default + Send + Sync;

    type SendWrapper: Send + Sync;
    type ReadWrapper;
    type WriteWrapper;
    fn wrap(value: Self::Read, at: Duration) -> Self::SendWrapper;
    fn convert_for_reading(wrapped: Self::SendWrapper, at: Duration) -> Self::ReadWrapper;
    fn convert_for_writing(wrapped: Self::ReadWrapper) -> Self::WriteWrapper;
    fn unwrap_write(wrapped: Self::WriteWrapper) -> Self::Write;
    fn unwrap_read(wrapped: Self::SendWrapper) -> Self::Read;

    type Sample;
    fn sample(value: &Self::ReadWrapper) -> Self::Sample;
}

pub trait Dynamic {
    type Sample: Copy;

    /// Samples the dynamic resource, given the times it was written and when
    /// it is being evaluated. NOT for human consumption.
    ///
    /// # Safety
    ///
    /// Calling this function within an operation gives the operation access
    /// to state that wasn't incorporated in the operation's hash. Meaning,
    /// if you call this function manually, the cache may give false POSITIVES.
    ///
    /// This function's implementations will likely never have any actual unsafe
    /// code. But since it has to be exposed publicly, I added `unsafe` to warn you.
    unsafe fn sample(&self, written: Time, now: Time) -> Self::Sample;
    fn evolve(&mut self, elapsed: Duration);
}

#[derive(Deref, DerefMut)]
pub struct Timestamped<T> {
    time: Time,
    #[deref]
    #[deref_mut]
    value: T,
}

impl<T> Timestamped<T> {
    pub fn new(time: Time, value: T) -> Self {
        Self { time, value }
    }

    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T: Dynamic> Timestamped<T> {
    pub fn new_from(other: Sampled<T>) -> Self {
        let mut value = other.value;
        value.evolve(other.now - other.written_at);
        Self {
            time: other.now,
            value,
        }
    }

    pub fn now(&self) -> T::Sample {
        unsafe { self.value.sample(self.time, self.time) }
    }

    pub fn evolve(&mut self, to: Time) {
        self.value.evolve(to - self.time);
        self.time = to;
    }
}

impl<T: Dynamic + Clone> Clone for Timestamped<T> {
    fn clone(&self) -> Self {
        Self {
            time: self.time,
            value: self.value.clone(),
        }
    }
}
impl<T: Dynamic + Copy> Copy for Timestamped<T> {}

pub struct Sampled<T: Dynamic> {
    written_at: Time,
    now: Time,
    value: T,
    sample: OnceCell<T::Sample>,
}

impl<T: Dynamic> Sampled<T> {
    pub fn new_from(timestamped: Timestamped<T>, now: Time) -> Self {
        Self {
            written_at: timestamped.time,
            now,
            value: timestamped.value,
            sample: OnceCell::new(),
        }
    }
}

impl<T: Dynamic> Deref for Sampled<T> {
    type Target = T::Sample;

    fn deref(&self) -> &Self::Target {
        unsafe {
            self.sample
                .get_or_init(|| self.value.sample(self.written_at, self.now))
        }
    }
}

#[macro_export]
macro_rules! hash_or {
    ($state:ident, $value:expr, $default:expr) => {
        let hashed = $crate::reexports::spez::spez! {
            for v = ($value, &mut $state);
            match<T: $crate::resource::Dynamic, H: std::hash::Hasher> ($crate::resource::Sampled<T>, &mut H) -> bool {
                false
            }
            match<T: std::hash::Hash, H: std::hash::Hasher> (T, &mut H) -> bool {
                v.0.hash(v.1);
                true
            }
            match<T: $crate::resource::PartialHash, H: std::hash::Hasher> (T, &mut H) -> bool {
                v.0.partial_hash(v.1);
                true
            }
            match<T, H: std::hash::Hasher> (T, &mut H) -> bool {
                false
            }
        };
        if !hashed {
            $default.hash(&mut $state);
        }
    };
}

pub trait PartialHash {
    fn partial_hash<H: Hasher>(&self, state: &mut H);
}

impl PartialHash for f32 {
    fn partial_hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.to_be_bytes());
    }
}

impl PartialHash for f64 {
    fn partial_hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.to_be_bytes());
    }
}

pub trait ResourceHistoryPlugin: Sync {
    fn write_type_string(&self) -> String;

    fn ser<'h>(&self, input: &'h TypeMap, type_map: &'h mut type_reg::untagged::TypeMap<String>);

    fn register(&self, type_reg: &mut TypeReg<String>);
    fn de<'h>(
        &self,
        output: &'h mut TypeMap,
        type_reg: &'h mut type_reg::untagged::TypeMap<String>,
    );
}

pub trait ErasedResource<'o>: 'o + Send + Sync {
    fn id(&self) -> u64;
}

impl<'o> dyn ErasedResource<'o> {
    pub(crate) unsafe fn downcast<TO: ErasedResource<'o>>(&self) -> &'o TO {
        unsafe { &*(self as *const Self as *const TO) }
    }

    pub(crate) unsafe fn downcast_mut<TO: ErasedResource<'o>>(&mut self) -> &'o mut TO {
        unsafe { &mut *(self as *mut Self as *mut TO) }
    }
    pub(crate) unsafe fn downcast_owned<TO: ErasedResource<'static> + Sized>(
        self: Box<Self>,
    ) -> Box<TO> {
        unsafe { Box::from_raw(Box::into_raw(self) as *mut TO) }
    }
}

impl<'o, R: Resource<'o>> ErasedResource<'o> for R {
    fn id(&self) -> u64 {
        Self::ID
    }
}
