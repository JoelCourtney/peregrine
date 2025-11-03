use std::{
    any::Any,
    cell::{Cell, UnsafeCell},
    mem::transmute,
    ops::Deref,
    rc::Rc,
};

use slotmap::{SlotMap, new_key_type};

use crate::{IncompatibleWorldErr, Run, node::Node};

new_key_type! { pub(crate) struct Key; }

#[derive(Default)]
pub struct World {
    inner: Cell<Option<Rc<WorldInner>>>,
}

impl From<&World> for World {
    fn from(world: &World) -> World {
        world.clone()
    }
}

impl Clone for World {
    fn clone(&self) -> World {
        self.init();
        let inner = self.inner.take();
        let new_inner = inner.clone();
        self.inner.set(inner);
        World {
            inner: Cell::new(new_inner),
        }
    }
}

impl World {
    pub fn new() -> Self {
        World::default()
    }

    fn init(&self) {
        let w = match self.inner.take() {
            Some(world) => world,
            None => Rc::new(WorldInner::new()),
        };
        self.inner.set(Some(w));
    }

    pub fn merge(self, other: World) -> Result<World, IncompatibleWorldErr> {
        self.merge_in_place(other)?;
        Ok(self)
    }

    pub fn merge_in_place(&self, other: World) -> Result<(), IncompatibleWorldErr> {
        let w1 = self.inner.take();
        if w1.is_none() {
            self.inner.set(other.inner.take());
            return Ok(());
        }

        let w2 = other.inner.into_inner();
        if w2.is_none() {
            self.inner.set(w1);
            return Ok(());
        }

        let (w1, w2) = (w1.unwrap(), w2.unwrap());

        if std::ptr::eq::<WorldInner>(&*w1, &*w2) {
            self.inner.set(Some(w1));
            Ok(())
        } else {
            Err(IncompatibleWorldErr)
        }
    }

    pub fn is_compatible(&self, other: &World) -> bool {
        let w1 = self.inner.take();
        let w2 = other.inner.take();

        let result = match (w1.as_ref(), w2.as_ref()) {
            (Some(w1), Some(w2)) => std::ptr::eq::<WorldInner>(&**w1, &**w2),
            _ => true,
        };

        self.inner.set(w1);
        other.inner.set(w2);

        result
    }
}

impl Deref for World {
    type Target = WorldInner;

    fn deref(&self) -> &Self::Target {
        self.init();
        unsafe { self.inner.as_ptr().as_ref().unwrap().as_deref().unwrap() }
    }
}

pub struct WorldInner {
    slots: UnsafeCell<SlotMap<Key, Box<dyn ErasedRun>>>,
}

pub(crate) trait AnyRun<O>: Any + Run<Output = O> {}
pub(crate) trait ErasedRun: Any + Run<Output = Never> {}

impl<O, N: Any + Run<Output = O>> AnyRun<O> for N {}

pub(crate) enum Never {}

#[derive(Copy, Clone)]
#[doc(hidden)]
pub struct WorldView<'e> {
    world: &'e WorldInner,
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

impl WorldInner {
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

impl Default for WorldInner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use peregrine_macros::op;

    use crate::{IntoRun, node::variable::Var};

    use super::*;
    use std::sync::Arc;

    use crate as peregrine;

    #[test]
    fn test_drop() {
        let arc = Arc::new(5);
        let w = World::new();

        assert_eq!(Arc::strong_count(&arc), 1);

        w.alloc(arc.clone().into_run().run);
        assert_eq!(Arc::strong_count(&arc), 2);

        drop(w);
        assert_eq!(Arc::strong_count(&arc), 1);
    }

    #[test]
    #[should_panic]
    fn incorrect_world() {
        let mut x = Var::new(0);
        let y = Var::new(1);
        
        x.set(y);
    }

    #[test]
    #[should_panic]
    fn incompatible_world() {
        let n1 = Var::new(5);
        let n2 = Var::new(10);

        let _ = op! { n1 + n2 };
    }
}
