use std::cell::UnsafeCell;

use parking_lot::{Mutex, MutexGuard};

static MASTER_LOCK: Mutex<()> = Mutex::new(());

pub(crate) struct SharedLock<T>(UnsafeCell<T>);

impl<T> SharedLock<T> {
    pub(crate) fn new(value: T) -> Self {
        SharedLock(UnsafeCell::new(value))
    }

    pub(crate) fn lock<'a>(&'a self, _key: &SharedLockKey<'a>) -> &'a mut T {
        unsafe { &mut *self.0.get() }
    }
}

unsafe impl<T> Sync for SharedLock<T> {}

#[expect(
    dead_code,
    reason = "The read guard is never used but it must be kept alive"
)]
pub(crate) struct SharedLockKey<'a>(MutexGuard<'a, ()>, Secret);

impl SharedLockKey<'_> {
    pub(crate) fn new() -> Self {
        SharedLockKey(MASTER_LOCK.lock(), Secret)
    }
}

struct Secret;
