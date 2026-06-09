use child_lock::parking_lot::MutexParent;

pub(crate) static PARENT: MutexParent = MutexParent::new();

pub(crate) type ChildLock<T> = child_lock::ChildLock<T, &'static MutexParent>;
