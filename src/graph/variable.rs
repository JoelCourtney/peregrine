use parking_lot::RwLock;

use crate::{Callback, Ctx, IntoUpstream, Upstream, cache::Cache};

use super::Node;

pub struct Var<O: Send + 'static> {
    node: Node<VarCell<O>>,
}

pub struct VarCell<O: Send + 'static> {
    cell: RwLock<Node<dyn Upstream<Output = O>>>,
    cache: Cache<()>,
}

impl<O: Send> Var<O> {
    pub fn new<U: Upstream<Output = O> + 'static>(node: impl IntoUpstream<U>) -> Var<O> {
        let outer = Node::empty();
        let inner = Node::new_dyn(node.into_upstream());
        outer.add_edge(&inner);
        let var_cell = outer.init(VarCell {
            cell: RwLock::new(inner),
            cache: Cache::new(),
        });
        Var { node: var_cell }
    }

    pub fn set<U: Upstream<Output = O> + 'static>(&mut self, node: impl IntoUpstream<U>) {
        let mut write = self.node.cell.write();
        self.node.remove_edge(&*write);
        let new_node = Node::new_dyn(node.into_upstream());
        self.node.add_edge(&new_node);
        self.node.cache.invalidate();
        *write = new_node;
    }

    pub fn freeze(&self) -> Node<dyn Upstream<Output = O>> {
        (*self.node.cell.read()).clone()
    }
}

impl<O: Send> Upstream for VarCell<O> {
    type Output = O;

    fn request(&self, ctx: Ctx, callback: Callback<O>) {
        let sender = self.cache.get_invalidator_sender();
        let callback = callback.map(|mut c| {
            c.push_sender(sender, false);
            c
        });
        let cell = self.cell.read();
        cell.request(ctx, callback);
    }
}

impl<O: Send> IntoUpstream<Node<VarCell<O>>> for Var<O> {
    fn into_upstream(self) -> Node<VarCell<O>> {
        self.node
    }
}

impl<O: Send> IntoUpstream<Node<VarCell<O>>> for &Var<O> {
    fn into_upstream(self) -> Node<VarCell<O>> {
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
        let x = op!(i!(1));
        // let y = Var::new(op! { x + 1 });

        assert_eq!(run(&x), 1);
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
