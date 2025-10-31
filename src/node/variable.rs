use std::cell::Cell;

use parking_lot::RwLock;

use crate::{
    Ctx, IntoRun, Run,
    cache::{Cache, MaybeCached},
    node::Node,
    world::World,
};

pub struct Var<'e, O: Send + 'static> {
    world: &'e World,
    node: Node<VarCell<O>>,
    frozen: Cell<bool>,
}

pub struct VarCell<O: Send + 'static> {
    cell: RwLock<Node<dyn Run<Output = O>>>,
    cache: Cache<()>,
}

impl<O: Send> Var<'_, O> {
    pub fn new<'e, N: Run<Output = O> + 'static>(
        world: &'e World,
        node: impl IntoRun<N>,
    ) -> Var<'e, O> {
        let current = world.alloc(node.into_run()).as_dyn();
        let var_node = VarCell {
            cell: RwLock::new(current),
            cache: Cache::new(),
        };
        Var::<'e, O> {
            world,
            node: world.alloc(var_node),
            frozen: Cell::new(false),
        }
    }

    pub fn world(&self) -> &World {
        self.world
    }

    pub fn set<N: Run<Output = O> + 'static>(&mut self, node: impl IntoRun<N>) {
        let var_node = self.world.get(self.node);
        var_node.cache.invalidate();
        let mut write = var_node.cell.write();
        if !self.frozen.get() {
            let current_key = *write;
            self.world.remove(current_key);
        } else {
            self.frozen.set(false);
        }
        *write = self.world.alloc(node.into_run()).as_dyn();
    }

    pub fn freeze(&self) -> Node<dyn Run<Output = O>> {
        self.frozen.set(true);
        let var_node = self.world.get(self.node);
        *var_node.cell.read()
    }
}

impl<O: Send> Run for VarCell<O> {
    type Output = O;

    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        let mut result = self.cell.read().run(ctx);
        result.push_sender(self.cache.get_invalidation_sender());
        result
    }
}

impl<O: Send> IntoRun<Node<dyn Run<Output = O>>> for Var<'_, O> {
    fn into_run(self) -> Node<dyn Run<Output = O>> {
        self.freeze()
    }
}

impl<O: Send> IntoRun<Node<VarCell<O>>> for &Var<'_, O> {
    fn into_run(self) -> Node<VarCell<O>> {
        self.node
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate as peregrine;
    use crate::*;

    #[test]
    fn var() {
        let w = World::new();
        let mut var = Var::new(&w, 0);
        assert_eq!(run(&w, &var), 0);

        var.set(7);
        assert_eq!(run(&w, &var), 7);
    }

    #[test]
    fn upstream_var() {
        let w = World::new();

        let x = Var::new(&w, 1);
        let y = Var::new(&w, op! { x + 1 });

        assert_eq!(run(&w, y), 2);
    }

    #[test]
    #[allow(clippy::drop_non_drop)]
    fn mutate_upstream_var() {
        let w = World::new();

        let mut x = Var::new(&w, 0);
        let y = op! { i!(&x) + 1 };

        assert_eq!(run(&w, &y), 1);

        x.set(10);
        drop(x);
        assert_eq!(run(&w, y), 11);
    }

    #[test]
    fn freeze() {
        let w = World::new();

        let mut x = Var::new(&w, 0);

        assert_eq!(run(&w, &x), 0);

        let frozen = x.freeze();
        x.set(10);
        assert_eq!(run(&w, x), 10);

        assert_eq!(run(&w, frozen), 0);
    }

    #[test]
    fn freeze_drop() {
        let w = World::new();

        let payload = Arc::new(());

        let mut x = Var::new(&w, payload.clone());
        assert_eq!(Arc::strong_count(&payload), 2);

        x.set(Arc::new(()));
        assert_eq!(Arc::strong_count(&payload), 1);

        x.set(payload.clone());
        x.freeze();
        assert_eq!(Arc::strong_count(&payload), 2);
        x.set(payload.clone());
        assert_eq!(Arc::strong_count(&payload), 3);

        drop(w);

        assert_eq!(Arc::strong_count(&payload), 1);
    }
}
