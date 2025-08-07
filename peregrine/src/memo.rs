#![doc(hidden)]

use ahash::AHasher;
use async_trait::async_trait;
use dashmap::DashMap;
use derive_more::Deref;
use std::{
    cell::RefCell,
    hash::{BuildHasher, Hash, Hasher},
};
use type_map::concurrent::TypeMap;

use crate::{Exec, Node};

#[async_trait]
pub trait Memoized {
    type System: Sync;
    type Input: Hash + Send;
    type Output: for<'h> Memo<'h>;
    const ID: u64;

    fn input(&self, sys: &Self::System) -> Self::Input;
    async fn run<'s>(&self, input: &Self::Input, env: Exec<'s>) -> Self::Output;
}

#[derive(Deref)]
pub struct MemoizedNode<'h, M: Memoized> {
    #[deref]
    node: M,
    memos: &'h InnerMemos<M::Output>,
}

#[async_trait]
impl<'h, M: Memoized + Send + Sync> Node for MemoizedNode<'h, M> {
    type System = M::System;
    type Output = <M::Output as Memo<'h>>::Read;

    async fn run<'s>(&self, sys: &'s Self::System, env: Exec<'s>) -> Self::Output {
        use std::hash::Hasher;

        let input = self.node.input(sys);
        let mut hasher = PeregrineDefaultHashBuilder::default();
        input.hash(&mut hasher);
        M::ID.hash(&mut hasher);
        let hash = hasher.finish();

        if let Some(o) = self.memos.get(hash) {
            o
        } else {
            let output = self.node.run(&input, env).await;
            self.memos.insert(hash, output)
        }
    }
}

pub trait Memo<'h>: Send + Sync + 'static {
    type Read;

    fn read(&self) -> Self::Read;
}

pub type PeregrineDefaultHashBuilder = AHasher;

#[derive(Default)]
#[repr(transparent)]
pub struct Memos(RefCell<TypeMap>);

impl Memos {
    pub fn new() -> Self {
        Memos(RefCell::new(TypeMap::new()))
    }
    pub fn memoize<M: Memoized>(&self, node: M) -> MemoizedNode<'_, M> {
        let mut map = self.0.borrow_mut();
        let inner: &InnerMemos<M::Output> = map.entry().or_insert_with(InnerMemos::default);
        let transmuted =
            unsafe { std::mem::transmute::<&InnerMemos<M::Output>, &InnerMemos<M::Output>>(inner) };
        MemoizedNode {
            node,
            memos: transmuted,
        }
    }
}

impl From<TypeMap> for Memos {
    fn from(value: TypeMap) -> Self {
        Memos(RefCell::new(value))
    }
}

/// See [Resource].
struct InnerMemos<T: for<'h> Memo<'h>>(DashMap<u64, T, PassThroughHashBuilder>);

impl<T: for<'h> Memo<'h>> Default for InnerMemos<T> {
    fn default() -> Self {
        InnerMemos(DashMap::with_hasher(PassThroughHashBuilder))
    }
}

impl<T: for<'h> Memo<'h>> InnerMemos<T> {
    fn insert<'h>(&self, hash: u64, value: T) -> <T as Memo<'h>>::Read {
        let inserted = self.0.entry(hash).or_insert(value);
        inserted.read()
    }

    fn get<'h>(&self, hash: u64) -> Option<<T as Memo<'h>>::Read> {
        self.0.get(&hash).map(move |r| r.value().read())
    }
}

// i suspect the compiler will be able to turn this into a no-op
pub struct PassThroughHasher(u64);

impl Hasher for PassThroughHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, _bytes: &[u8]) {
        unreachable!()
    }
    fn write_u8(&mut self, _i: u8) {
        unreachable!()
    }
    fn write_u16(&mut self, _i: u16) {
        unreachable!()
    }
    fn write_u32(&mut self, _i: u32) {
        unreachable!()
    }

    fn write_u64(&mut self, i: u64) {
        self.0 = i;
    }

    fn write_usize(&mut self, _i: usize) {
        unreachable!()
    }
}

#[derive(Copy, Clone, Default)]
pub struct PassThroughHashBuilder;

impl BuildHasher for PassThroughHashBuilder {
    type Hasher = PassThroughHasher;

    fn build_hasher(&self) -> PassThroughHasher {
        PassThroughHasher(0)
    }
}

impl Memo<'_> for usize {
    type Read = Self;

    fn read(&self) -> Self::Read {
        *self
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;

    use async_trait::async_trait;

    use crate::{
        Exec,
        memo::{Memoized, Memos},
    };

    #[test]
    fn memo() {
        struct A(AtomicU32);

        #[async_trait]
        impl Memoized for A {
            type System = ();
            type Input = usize;
            type Output = usize;
            const ID: u64 = 1;

            fn input(&self, _sys: &Self::System) -> Self::Input {
                5
            }

            async fn run<'s>(&self, input: &Self::Input, _env: Exec<'s>) -> Self::Output {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                input + 1
            }
        }

        let memos = Memos::new();
        let a1 = memos.memoize(A(AtomicU32::new(0)));
        let a2 = memos.memoize(A(AtomicU32::new(0)));

        assert_eq!(a1.0.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(a2.0.load(std::sync::atomic::Ordering::SeqCst), 0);

        assert_eq!(Exec::run_blocking(&(), &a1), 6);
        assert_eq!(Exec::run_blocking(&(), &a2), 6);

        assert_eq!(a1.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(a2.0.load(std::sync::atomic::Ordering::SeqCst), 0);

        assert_eq!(Exec::run_blocking(&(), &a1), 6);
        assert_eq!(Exec::run_blocking(&(), &a2), 6);

        assert_eq!(a1.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(a2.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}
