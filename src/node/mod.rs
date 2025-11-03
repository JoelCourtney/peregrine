// pub mod stack;
pub mod auto;
pub mod variable;

use crate::{
    Ctx, IntoRun, Run,
    cache::MaybeCached,
    world::{Key, World, WorldId},
};

pub struct Node<T: ?Sized> {
    pub(crate) index: Key,
    pub(crate) phantom: std::marker::PhantomData<T>,
    pub(crate) world_id: WorldId,
}

impl<R: Run> Node<R> {
    pub fn new(world: &World, run: impl IntoRun<R>) -> Self
    where
        R: 'static,
    {
        world.alloc(run.into_run())
    }

    pub fn as_dyn(&self) -> Node<dyn Run<Output = R::Output>> {
        Node {
            index: self.index,
            phantom: std::marker::PhantomData,
            world_id: self.world_id,
        }
    }
}

impl<R: Run + 'static> Run for Node<R> {
    type Output = R::Output;

    fn world_id(&self) -> WorldId {
        self.world_id
    }
    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        ctx.world.get(*self).run(ctx)
    }
}

impl<O: Send + 'static> Run for Node<dyn Run<Output = O>> {
    type Output = O;

    fn world_id(&self) -> WorldId {
        self.world_id
    }
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
