use std::cell::Cell;

use parking_lot::RwLock;

use crate::{
    Ctx, IntoRun, Run, RunInWorld,
    cache::{Cache, MaybeCached},
    node::Node,
    world::World,
};

pub struct Var<O: Send + 'static> {
    world: World,
    node: Node<VarCell<O>>,
    frozen: Cell<bool>,
}

pub struct VarCell<O: Send + 'static> {
    cell: RwLock<Node<dyn Run<Output = O>>>,
    cache: Cache<()>,
}

impl<O: Send> Var<O> {
    pub fn new<N: Run<Output = O> + 'static>(node: impl IntoRun<N>) -> Var<O> {
        let RunInWorld { run, world } = node.into_run();
        let current = world.alloc(run).as_dyn();
        let var_node = VarCell {
            cell: RwLock::new(current),
            cache: Cache::new(),
        };
        Var {
            node: world.alloc(var_node),
            world,
            frozen: Cell::new(false),
        }
    }

    pub fn world(&self) -> World {
        self.world.clone()
    }

    pub fn set<N: Run<Output = O> + 'static>(&mut self, node: impl IntoRun<N>) {
        let RunInWorld {
            run,
            world: new_world,
        } = node.into_run();
        self.world
            .merge_in_place(new_world)
            .expect("Cannot set variable to a node that requires a different world");
        let var_node = self.world.get(self.node);
        var_node.cache.invalidate();
        let mut write = var_node.cell.write();
        if !self.frozen.get() {
            let current_key = *write;
            self.world.remove(current_key);
        } else {
            self.frozen.set(false);
        }
        *write = self.world.alloc(run).as_dyn();
    }

    pub fn freeze(&self) -> RunInWorld<Node<dyn Run<Output = O>>> {
        self.frozen.set(true);
        let var_node = self.world.get(self.node);
        RunInWorld::new(*var_node.cell.read(), self.world.clone())
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

impl<O: Send> IntoRun<Node<dyn Run<Output = O>>> for Var<O> {
    fn into_run(self) -> RunInWorld<Node<dyn Run<Output = O>>> {
        let r = *self.world.get(self.node).cell.read();
        RunInWorld::new(r, self.world)
    }
}

impl<O: Send> IntoRun<Node<dyn Run<Output = O>>> for &Var<O> {
    fn into_run(self) -> RunInWorld<Node<dyn Run<Output = O>>> {
        RunInWorld::new(self.node.as_dyn(), self.world.clone())
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
        let mut var = Var::new(0);
        assert_eq!(run(&var), Ok(0));

        var.set(7);
        assert_eq!(run(&var), Ok(7));
    }

    #[test]
    fn upstream_var() {
        let x = Var::new(1);
        let y = Var::new(op! { x + 1 });

        assert_eq!(run(&y), Ok(2));
    }

    #[test]
    #[allow(clippy::drop_non_drop)]
    fn mutate_upstream_var() {
        let mut x = Var::new(0);
        let y = op! { i!(&x) + 1 };

        assert_eq!(run(&y), Ok(1));

        x.set(10);
        drop(x);
        assert_eq!(run(y), Ok(11));
    }

    #[test]
    fn freeze() {
        let mut x = Var::new(0);

        assert_eq!(run(&x), Ok(0));

        let frozen = x.freeze();
        x.set(10);
        assert_eq!(run(&x), Ok(10));

        assert_eq!(run(frozen), Ok(0));
    }

    #[test]
    fn freeze_drop() {
        let payload = Arc::new(());

        let mut x = Var::new(payload.clone());
        assert_eq!(Arc::strong_count(&payload), 2);

        x.set(Arc::new(()));
        assert_eq!(Arc::strong_count(&payload), 1);

        x.set(payload.clone());
        x.freeze();
        assert_eq!(Arc::strong_count(&payload), 2);
        x.set(payload.clone());
        assert_eq!(Arc::strong_count(&payload), 3);

        drop(x);

        assert_eq!(Arc::strong_count(&payload), 1);
    }
}
