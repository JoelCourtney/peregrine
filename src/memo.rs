#![doc(hidden)]

use ahash::AHasher;
use forte::Worker;
use quick_cache::{UnitWeighter, sync::Cache};
use std::{
    cell::RefCell,
    hash::{BuildHasher, Hash, Hasher},
};
use type_map::concurrent::TypeMap;

use crate::{
    Data, Node,
    cache::{CacheableNode, UpstreamReceiver},
};

pub trait MemoizeableNode: Send + Sync {
    type Input: Hash + Send;
    type Context;
    type Output: Data;
    const CACHE_SIZE: usize;

    fn input(&self, ctx: &mut Self::Context) -> Self::Input;
    fn run(input: &Self::Input, s: &Worker) -> Self::Output;
}

pub struct MemoizedNode<'h, M: MemoizeableNode> {
    node: M,
    memos: &'h InnerMemos<M>,
}

impl<'h, M: MemoizeableNode<Context = ()>> Node for MemoizedNode<'h, M> {
    type Output = M::Output;

    fn run(&self, w: &Worker) -> Self::Output {
        self.run_with_context(&mut (), w)
    }
}

impl<'h, O: Send + Sync, M: MemoizeableNode<Output = O, Context = UpstreamReceiver<O>>>
    CacheableNode for MemoizedNode<'h, M>
{
    type Output = M::Output;

    fn run_with_receiver(
        &self,
        w: &Worker,
        r: &mut UpstreamReceiver<Self::Output>,
    ) -> Self::Output {
        self.run_with_context(r, w)
    }
}

impl<'h, M: MemoizeableNode> MemoizedNode<'h, M> {
    #[allow(unused_must_use)]
    fn run_with_context(&self, ctx: &mut M::Context, w: &Worker) -> M::Output {
        use std::hash::Hasher;

        let input = self.node.input(ctx);
        let mut hasher = PeregrineDefaultHashBuilder::default();
        input.hash(&mut hasher);
        let hash = hasher.finish();

        match w.block_on(self.memos.get_value_or_guard_async(&hash)) {
            Ok(v) => v,
            Err(g) => {
                let output = M::run(&input, w);
                g.insert(output.clone());
                output
            }
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
    pub fn memoize<M: MemoizeableNode + 'static>(&self, node: M) -> MemoizedNode<'_, M> {
        let mut map = self.0.borrow_mut();
        let inner: &InnerMemos<M> = map.entry().or_insert_with(|| {
            InnerMemos::<M>::with(
                M::CACHE_SIZE,
                M::CACHE_SIZE as u64,
                Default::default(),
                Default::default(),
                Default::default(),
            )
        });
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

type InnerMemos<M> =
    Cache<u64, <M as MemoizeableNode>::Output, UnitWeighter, PassThroughHashBuilder>;

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

#[cfg(test)]
mod tests {
    use forte::Worker;

    use crate::{
        memo::{MemoizeableNode, Memos},
        run,
    };
    use std::sync::atomic::AtomicU32;

    #[test]
    fn memo() {
        static COUNTER: AtomicU32 = AtomicU32::new(0);

        struct A;

        impl MemoizeableNode for A {
            type Input = usize;
            type Context = ();
            type Output = usize;
            const CACHE_SIZE: usize = 10;

            fn input(&self, _: &mut ()) -> Self::Input {
                5
            }

            fn run(input: &Self::Input, _: &Worker) -> Self::Output {
                COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                input + 1
            }
        }

        let memos = Memos::new();
        let a1 = memos.memoize(A);
        let a2 = memos.memoize(A);

        assert_eq!(COUNTER.load(std::sync::atomic::Ordering::SeqCst), 0);

        assert_eq!(run(&a1), 6);
        assert_eq!(run(&a2), 6);

        assert_eq!(COUNTER.load(std::sync::atomic::Ordering::SeqCst), 1);

        assert_eq!(run(&a1), 6);
        assert_eq!(run(&a2), 6);

        assert_eq!(COUNTER.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
