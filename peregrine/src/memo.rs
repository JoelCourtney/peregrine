#![doc(hidden)]

use crate::{Exec, Node, read::Readable};
use ahash::AHasher;
use async_trait::async_trait;
use dashmap::DashMap;
use derive_more::Deref;
use std::{
    cell::RefCell,
    hash::{BuildHasher, Hash, Hasher},
};
use type_map::concurrent::TypeMap;

#[async_trait]
pub trait Memoized {
    type Input: Hash + Send;
    type Output: Readable;

    fn input(&self) -> Self::Input;
    async fn run<'s>(&self, input: &Self::Input, env: Exec<'s>) -> Self::Output;
}

#[derive(Deref)]
pub struct MemoizedNode<'h, M: Memoized> {
    #[deref]
    node: M,
    memos: &'h InnerMemos<M>,
}

#[async_trait]
impl<'h, M: Memoized + Send + Sync> Node for MemoizedNode<'h, M> {
    type Output = <M::Output as Readable>::Read;

    async fn run<'s>(&self, env: Exec<'s>) -> Self::Output {
        use std::hash::Hasher;

        let input = self.node.input();
        let mut hasher = PeregrineDefaultHashBuilder::default();
        input.hash(&mut hasher);
        let hash = hasher.finish();

        if let Some(o) = self.memos.get(hash) {
            o
        } else {
            let output = self.node.run(&input, env).await;
            self.memos.insert(hash, output)
        }
    }
}

pub type PeregrineDefaultHashBuilder = AHasher;

#[derive(Default)]
#[repr(transparent)]
pub struct Memos(RefCell<TypeMap>);

impl Memos {
    pub fn new() -> Self {
        Memos(RefCell::new(TypeMap::new()))
    }
    pub fn memoize<M: Memoized + 'static>(&self, node: M) -> MemoizedNode<'_, M> {
        let mut map = self.0.borrow_mut();
        let inner: &InnerMemos<M> = map.entry().or_insert_with(InnerMemos::default);
        let transmuted = unsafe { std::mem::transmute::<&InnerMemos<M>, &InnerMemos<M>>(inner) };
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
struct InnerMemos<M: Memoized>(DashMap<u64, M::Output, PassThroughHashBuilder>);

impl<M: Memoized> Default for InnerMemos<M> {
    fn default() -> Self {
        InnerMemos(DashMap::with_hasher(PassThroughHashBuilder))
    }
}

impl<M: Memoized> InnerMemos<M> {
    fn insert(&self, hash: u64, value: M::Output) -> <M::Output as Readable>::Read {
        let inserted = self.0.entry(hash).or_insert(value);
        inserted.read()
    }

    fn get(&self, hash: u64) -> Option<<M::Output as Readable>::Read> {
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

impl Readable for usize {
    type Read = Self;

    fn read(&self) -> Self::Read {
        *self
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        Exec,
        memo::{Memoized, Memos},
    };
    use async_trait::async_trait;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn memo() {
        struct A(AtomicU32);

        #[async_trait]
        impl Memoized for A {
            type Input = usize;
            type Output = usize;

            fn input(&self) -> Self::Input {
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

        assert_eq!(Exec::run_blocking(&a1), 6);
        assert_eq!(Exec::run_blocking(&a2), 6);

        assert_eq!(a1.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(a2.0.load(std::sync::atomic::Ordering::SeqCst), 0);

        assert_eq!(Exec::run_blocking(&a1), 6);
        assert_eq!(Exec::run_blocking(&a2), 6);

        assert_eq!(a1.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(a2.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}
