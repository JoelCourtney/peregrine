use std::{
    any::Any,
    cell::UnsafeCell,
    mem::transmute,
    ops::{Deref, DerefMut},
};

use slotmap::{SlotMap, new_key_type};

use crate::{node::Node, IntoRun, Run};

new_key_type! { pub(crate) struct Key; }

#[must_use]
pub struct World {
    slots: UnsafeCell<SlotMap<Key, Box<dyn ErasedRun>>>,
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
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WithWorld<T> {
    world: Box<World>,
    data: T,
}

impl<T> WithWorld<T> {
    pub fn init(f: impl FnOnce(&'static World) -> T) -> Self {
        let world = Box::new(World::new());
        let world_ref = unsafe {
            transmute::<&World, &'static World>(&*world)
        };
        WithWorld {
            world,
            data: f(world_ref)
        }
    }
    
    pub fn world(&self) -> &World {
        &self.world
    }
    
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> WithWorld<U> {
        WithWorld {
            world: self.world,
            data: f(self.data)
        }
    }
}

impl<T> Deref for WithWorld<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> DerefMut for WithWorld<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

impl<'a, T, R: Run> IntoRun<R> for &'a WithWorld<T> where &'a T: IntoRun<R> {
    fn into_run(self) -> R {
        self.deref().into_run()
    }
}

#[cfg(test)]
mod tests {
    use crate::{node::variable::Var, run, IntoRun};

    use super::*;
    use std::sync::Arc;

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
        let mut var = WithWorld::init(|w| {
            Var::new(w, 5)
        });

        assert_eq!(run(var.world(), &var), 5);

        var.set(10);
        
        let asdf = (&var).into_run();
        
        assert_eq!(run(var.world(), &var), 10);
    }
}
