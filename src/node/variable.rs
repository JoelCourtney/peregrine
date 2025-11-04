use std::sync::RwLock;

use crate::{
    Ctx, IntoRun, Run,
    cache::{Cache, MaybeCached},
};

use super::Node;

pub struct Var<O: Send + 'static> {
    node: Node<VarCell<O>>,
}

pub struct VarCell<O: Send + 'static> {
    cell: RwLock<Node<dyn Run<Output = O>>>,
    cache: Cache<()>,
}

impl<O: Send> Var<O> {
    pub fn new<R: Run<Output = O> + 'static>(node: impl IntoRun<R>) -> Var<O> {
        let outer = Node::empty();
        let inner = Node::new_dyn(node.into_run());
        outer.add_edge(&inner);
        let var_cell = outer.init(VarCell {
            cell: RwLock::new(inner),
            cache: Cache::new(),
        });
        Var { node: var_cell }
    }

    pub fn set<N: Run<Output = O> + 'static>(&mut self, node: impl IntoRun<N>) {
        let mut write = self.node.cell.write().unwrap();
        self.node.remove_edge(&*write);
        let new_node = Node::new_dyn(node.into_run());
        self.node.add_edge(&new_node);
        self.node.cache.invalidate();
        *write = new_node;
    }

    pub fn freeze(&self) -> Node<dyn Run<Output = O>> {
        (*self.node.cell.read().unwrap()).clone()
    }
}

impl<O: Send> Run for VarCell<O> {
    type Output = O;

    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        let mut result = self.cell.read().unwrap().run(ctx);
        result.push_sender(self.cache.get_invalidation_sender());
        result
    }
}

impl<O: Send> IntoRun<Node<VarCell<O>>> for Var<O> {
    fn into_run(self) -> Node<VarCell<O>> {
        self.node
    }
}

impl<O: Send> IntoRun<Node<VarCell<O>>> for &Var<O> {
    fn into_run(self) -> Node<VarCell<O>> {
        self.node.clone()
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
        assert_eq!(run(&var), 0);

        var.set(7);
        assert_eq!(run(&var), 7);
    }

    #[test]
    fn upstream_var() {
        let x = Var::new(1);
        let y = Var::new(op! { x + 1 });

        assert_eq!(run(&y), 2);
    }

    #[test]
    #[allow(clippy::drop_non_drop)]
    fn mutate_upstream_var() {
        let mut x = Var::new(0);
        let y = op! { i!(&x) + 1 };

        assert_eq!(run(&y), 1);

        x.set(10);
        drop(x);
        assert_eq!(run(y), 11);
    }

    #[test]
    fn freeze() {
        let mut x = Var::new(0);

        assert_eq!(run(&x), 0);

        let frozen = x.freeze();
        x.set(10);
        assert_eq!(run(&x), 10);

        assert_eq!(run(frozen), 0);
    }

    #[test]
    fn freeze_drop() {
        let payload = Arc::new(());

        let mut x = Var::new(payload.clone());
        assert_eq!(Arc::strong_count(&payload), 2);

        x.set(Arc::new(()));
        assert_eq!(Arc::strong_count(&payload), 1);

        x.set(payload.clone());
        let frozen = x.freeze();
        assert_eq!(Arc::strong_count(&payload), 2);
        x.set(payload.clone());
        assert_eq!(Arc::strong_count(&payload), 3);

        drop(frozen);
        drop(x);

        assert_eq!(Arc::strong_count(&payload), 1);
    }
}
