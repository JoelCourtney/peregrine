//! User-facing resource type implementations.
//!
//! This module contains built-in resource types that users can use directly
//! in their models and activities.

pub mod builtins;
pub mod piecewise;
pub mod polynomial;
pub mod timer;

// Re-export commonly used types for convenience
pub use builtins::{elapsed, now};
pub use piecewise::Piecewise;
pub use polynomial::{Linear, Polynomial, Quadratic};
pub use timer::Stopwatch;

// Re-export the init function for internal use
use crate::Time;
pub(crate) use builtins::init_builtins_timelines;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::hash::Hasher;

/// Allows a type to be stored and operated on by peregrine.
///
/// All types used by resources implement this trait.
///
/// I intend to provide a derive macro to make this easier, so for now
/// I'm not going to go into a lot of detail on how to implement this.
pub trait Data<'h>:
    'static + MaybeHash + Clone + Serialize + DeserializeOwned + Send + Sync
{
    /// The type sent from upstream nodes to their downstream nodes.
    ///
    /// This type is read out of history in potentially a different form than it
    /// was written in. This is because you are not allowed to keep a reference
    /// to data directly stored in the history. The history is a hash map that might
    /// be resized at any time, invalidating those references. Double indirect references
    /// are fine though.
    type Read: 'h + Copy + Send + Sync;

    /// The type provided to operations that only read from this resource.
    ///
    /// This may be equal to the original [Data] type, or might not, depending
    /// on how much information you want to give to operations. The trade-off
    /// is that the more information an operation is given, the less lower its
    /// cache rate will be.
    ///
    /// All information in the sample type that could impact the output of an
    /// operation in any way must be included in the [MaybeHash] implementation.
    type Sample: 'h + MaybeHash;

    /// Convert a stored history value to the [Self::Read] type, given the time
    /// that it was written to history. Avoid cloning data if at all possible.
    fn to_read(&self, written: Time) -> Self::Read;

    /// Convert the [Self::Read] type back into an owned instance of the original
    /// data. Cloning the data might be necessary.
    ///
    /// This function is given the current time, which might be used for dynamic
    /// resources to evolve the data further in time. For example, imagine a simple
    /// `Line` type that stores only an `initial_value` and `slope`. It doesn't store
    /// the time the line starts at, so that the cache can be reused at different times.
    /// It blindly assumes that the line starts at whatever the current time is.
    ///
    /// However, this means that [from_read] will need to evolve the line further in
    /// time for it to be valid the next time it is written to. So, [from_read] would
    /// not just create a new copy, it would add `slope * (now - written).to_seconds()`
    /// to `initial_value`.
    fn from_read(read: Self::Read, now: Time) -> Self;

    /// Create a sample from the [Self::Read] value and the current time. Like [from_read],
    /// this function will need to provide an interface that is evolved in time to `now`.
    /// Unlike [from_read], you should try to do that without cloning or mutating any data.
    fn sample(read: Self::Read, now: Time) -> Self::Sample;
}

/// Marks a type as a resource label.
///
/// There are almost no practical uses to implementing this trait manually.
/// Its better to use the [resource][crate::resource!] macro.
pub trait Resource: 'static + Sync + Copy {
    /// A stringified version of the resource name.
    const LABEL: &'static str;

    /// A unique identifier for this resource - NOT stable between compilations.
    const ID: u64;

    /// The type that is written from operations to history.
    type Data: for<'h> Data<'h>;

    const INSTANCE: Self;

    /// Returns the initial condition for this resource.
    /// This is used when no explicit initial condition is provided in the model.
    /// Returns None if no default value was specified in the resource declaration.
    fn initial_condition() -> Option<Self::Data>;
}

/// A trait for data that might or might not be hashable.
///
/// This is used for caching; being able to hash inputs might increase the
/// cache hit rate dramatically.
pub trait MaybeHash {
    /// Whether this data is hashable. For most types, this will always
    /// be either true or false, but some (like those that contain floats)
    /// might not be hashable for some values.
    fn is_hashable(&self) -> bool;

    /// Hash the value, given that [MaybeHash::is_hashable] was already check and returned true.
    ///
    /// Unhashable data types should panic if this method is called.
    fn hash_unchecked<H: Hasher>(&self, state: &mut H);
}
