use std::cell::UnsafeCell;

use parking_lot::{Mutex, MutexGuard};

static MASTER_LOCK: Mutex<()> = Mutex::new(());

/// A mutex that shares a lock with all other instances of [`SharedLock`].
///
/// To unlock it, you must get the [`SharedKey`] with [`SharedKey::new`].
/// There can only be one instance of [`SharedKey`] at a time, and
/// [`SharedKey::new`] blocks until any existing instance is dropped.
/// Once you have the key, unlocking [`SharedLock`]s is a no-op.
/// You can unlock one lock mutably at a time, or many locks immutably
/// simulataneously.
///
/// ## Why?
///
/// 1. The the unlocked reference can live as long as your reference
///    to the lock itself, as long as the key lives longer. This is different
///    from a regular mutex, which shortens the unlocked reference to live
///    no longer than the mutex guard, which itself lives shorter than
///    your reference to the lock. This features is what allows the `request`
///    functions to take `&'s self` arguments, even though there are mutexes
///    in the way. Otherwise there would be several extra transmutes throughout
///    the code.
/// 2. As a bonus, simulation is more performant. A large chunk of the engine
///    overhead is fiddling with mutexes and atomics. Unlocking a [`SharedLock`]
///    is a no-op, and you only have to get the key once per simulation.
///
/// ## SAFETY
///
/// This is safe because there can only be one [`SharedKey`] at a time. You can
/// only unlock one lock mutably at a time, because it takes `&mut SharedKey` to
/// do so, or you can unlock many immutably at a time.
pub(crate) struct SharedLock<T>(UnsafeCell<T>);

impl<T> SharedLock<T> {
    pub(crate) fn new(value: T) -> Self {
        SharedLock(UnsafeCell::new(value))
    }

    #[inline]
    pub(crate) fn read(&self, _key: &SharedKey) -> &T {
        // SAFETY: see [SharedLock]
        unsafe { &*self.0.get() }
    }

    #[inline]
    #[expect(
        clippy::mut_from_ref,
        reason = "that's the whole point of interior mutability"
    )]
    pub(crate) fn write(&self, _key: &mut SharedKey) -> &mut T {
        // SAFETY: see [SharedLock]
        unsafe { &mut *self.0.get() }
    }
}

// SAFETY: see [SharedLock]
unsafe impl<T> Sync for SharedLock<T> {}

#[expect(
    dead_code,
    reason = "The read guard is never used but it must be kept alive"
)]
pub(crate) struct SharedKey<'a>(MutexGuard<'a, ()>);

impl SharedKey<'_> {
    pub(crate) fn new() -> Self {
        SharedKey(MASTER_LOCK.lock())
    }
}
