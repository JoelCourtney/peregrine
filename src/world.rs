use std::{
    any::Any,
    cell::UnsafeCell,
    mem::transmute,
    ops::{Deref, DerefMut},
    sync::atomic::AtomicU32,
};

use slotmap::{SlotMap, new_key_type};

use crate::{IntoRun, Run, node::Node};

new_key_type! { pub(crate) struct Key; }

#[must_use]
pub struct World {
    slots: UnsafeCell<SlotMap<Key, Box<dyn ErasedRun>>>,
    id: WorldId,
}

pub trait InWorld {
    fn world(&self) -> &World;
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub enum WorldId {
    Any,
    Only(u32),
}

impl WorldId {
    pub(crate) fn new_unique() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        Self::Only(COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }

    pub fn any() -> Self {
        Self::Any
    }

    pub(crate) fn matches(&self, other: &Self) -> bool {
        use WorldId::*;

        match (self, other) {
            (Any, _) | (_, Any) => true,
            (id1, id2) => id1 == id2,
        }
    }

    pub fn merge(self, other: Self) -> Option<Self> {
        use WorldId::*;

        match (self, other) {
            (Any, o) | (o, Any) => Some(o),
            (id1, id2) if id1 == id2 => Some(id1),
            _ => None,
        }
    }
}

pub(crate) trait AnyRun<O>: Any + Run<Output = O> {}
pub(crate) trait ErasedRun: Any + Run<Output = Never> {}

impl<O, N: Any + Run<Output = O>> AnyRun<O> for N {}

pub(crate) enum Never {}

#[derive(Copy, Clone)]
#[doc(hidden)]
pub struct WorldView<'e> {
    world: &'e World,
}

impl WorldView<'_> {
    pub fn get<T: 'static>(&self, node: Node<T>) -> &T {
        self.world.get(node)
    }

    pub fn get_dyn<O>(&self, key: Node<dyn Run<Output = O>>) -> &dyn Run<Output = O> {
        self.world.get_dyn(key)
    }
}

unsafe impl Sync for WorldView<'_> {}

impl World {
    pub fn new() -> Self {
        Self {
            slots: UnsafeCell::new(SlotMap::with_key()),
            id: WorldId::new_unique(),
        }
    }

    pub(crate) fn alloc<O, N: AnyRun<O>>(&self, value: N) -> Node<N> {
        let b = Box::new(value) as Box<dyn AnyRun<O>>;
        let b_erased = unsafe { transmute::<Box<dyn AnyRun<O>>, Box<dyn ErasedRun>>(b) };
        let index = unsafe { self.slots.get().as_mut() }
            .unwrap()
            .insert(b_erased);
        Node {
            index,
            phantom: std::marker::PhantomData,
            world_id: self.id,
        }
    }

    pub(crate) fn get<T: 'static>(&self, key: Node<T>) -> &T {
        use std::any::Any;

        let slot = unsafe { self.slots.get().as_ref() }
            .unwrap()
            .get(key.index)
            .expect("Key not found");

        (&**slot as &dyn Any)
            .downcast_ref()
            .expect("Incompatible type cast")
    }

    pub(crate) fn get_dyn<O>(&self, key: Node<dyn Run<Output = O>>) -> &dyn Run<Output = O> {
        let slot = unsafe { self.slots.get().as_ref() }
            .unwrap()
            .get(key.index)
            .expect("Key not found");
        unsafe { transmute::<&dyn ErasedRun, &dyn AnyRun<O>>(&**slot) }
    }

    pub(crate) fn remove<T: ?Sized>(&self, key: Node<T>) {
        unsafe { self.slots.get().as_mut() }
            .unwrap()
            .remove(key.index);
    }

    pub(crate) unsafe fn view(&self) -> WorldView<'_> {
        WorldView { world: self }
    }

    pub(crate) fn id(&self) -> WorldId {
        self.id
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WithWorld<'w, T> {
    world: WorldHandle<'w>,
    data: T,
}

impl<T> InWorld for WithWorld<'_, T> {
    fn world(&self) -> &World {
        match &self.world {
            WorldHandle::Owned(world) => world,
            WorldHandle::Borrowed(world) => world,
        }
    }
}

enum WorldHandle<'w> {
    Owned(Box<World>),
    Borrowed(&'w World),
}

impl Deref for WorldHandle<'_> {
    type Target = World;

    fn deref(&self) -> &Self::Target {
        match self {
            WorldHandle::Owned(world) => world,
            WorldHandle::Borrowed(world) => world,
        }
    }
}

impl<T> WithWorld<'static, T> {
    #[allow(unused)]
    pub(crate) unsafe fn owned(f: impl FnOnce(&'static World) -> T) -> Self {
        let world = WorldHandle::Owned(Box::default());
        let world_ref = unsafe { transmute::<&World, &'static World>(&*world) };
        WithWorld {
            world,
            data: f(world_ref),
        }
    }
}

impl<'w, T> WithWorld<'w, T> {
    pub fn borrowed(world: &'w World, data: T) -> WithWorld<'w, T> {
        WithWorld {
            world: WorldHandle::Borrowed(world),
            data,
        }
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> WithWorld<'w, U> {
        WithWorld {
            world: self.world,
            data: f(self.data),
        }
    }

    pub fn into_inner(self) -> T
    where
        T: 'static,
    {
        self.data
    }
}

impl<T> Deref for WithWorld<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> DerefMut for WithWorld<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

impl<T, R: Run> IntoRun<R> for WithWorld<'_, T>
where
    T: IntoRun<R>,
{
    fn into_run(self) -> R {
        self.data.into_run()
    }
}

impl<'a, T, R: Run> IntoRun<R> for &'a WithWorld<'_, T>
where
    &'a T: IntoRun<R>,
{
    fn into_run(self) -> R {
        self.deref().into_run()
    }
}

#[cfg(test)]
mod tests {
    use peregrine_macros::op;

    use crate::{IncompatibleWorldErr, IntoRun, node::variable::Var, run, run_in};

    use super::*;
    use std::sync::Arc;

    use crate as peregrine;

    #[test]
    fn test_drop() {
        let arc = Arc::new(5);
        let w = World::new();

        assert_eq!(Arc::strong_count(&arc), 1);

        w.alloc(arc.clone().into_run());
        assert_eq!(Arc::strong_count(&arc), 2);

        drop(w);
        assert_eq!(Arc::strong_count(&arc), 1);
    }

    #[test]
    fn test_with_world() {
        let mut var = unsafe { WithWorld::owned(|w| Var::new(w, 5)) };

        assert_eq!(run(&var), Ok(5));

        var.set(10);

        assert_eq!(run(&var), Ok(10));
    }

    #[test]
    fn incorrect_world() {
        let w = World::new();
        let node = Node::new(&w, 5);

        assert_eq!(run_in(&w, &node), Ok(5));
        assert_eq!(run_in(&World::new(), &node), Err(IncompatibleWorldErr));
    }

    #[test]
    #[should_panic]
    fn incompatible_world() {
        let w1 = World::new();
        let w2 = World::new();
        let n1 = Node::new(&w1, 5);
        let n2 = Node::new(&w2, 10);

        let _ = op! { n1 + n2 };
    }
}
