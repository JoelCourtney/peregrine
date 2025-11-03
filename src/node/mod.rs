pub mod auto;
pub mod op;
pub mod stack;
pub mod variable;

use crate::{Ctx, Run, cache::MaybeCached, world::Key};

pub struct Node<T: ?Sized> {
    pub(crate) index: Key,
    pub(crate) phantom: std::marker::PhantomData<T>,
}

impl<R: Run> Node<R> {
    pub fn as_dyn(&self) -> Node<dyn Run<Output = R::Output>> {
        Node {
            index: self.index,
            phantom: std::marker::PhantomData,
        }
    }
}

impl<R: Run + 'static> Run for Node<R> {
    type Output = R::Output;

    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        ctx.world.get(*self).run(ctx)
    }
}

impl<O: Send + 'static> Run for Node<dyn Run<Output = O>> {
    type Output = O;

    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        ctx.world.get_dyn(*self).run(ctx)
    }
}

impl<T: ?Sized> Clone for Node<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for Node<T> {}
