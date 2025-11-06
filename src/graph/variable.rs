use std::sync::Arc;

use parking_lot::RwLock;

use crate::{Callback, Ctx, IntoUpstream, Upstream, cache::Cache};

use super::Node;

pub struct Var<O: Send + 'static> {
    node: Arc<VarCell<O>>,
}

pub struct VarCell<O: Send + 'static> {
    node: Node,
    cell: RwLock<Arc<dyn Upstream<Output = O>>>,
    cache: Cache<()>,
}

impl<O: Send> Var<O> {
    pub fn new<U: Upstream<Output = O> + 'static>(node: impl IntoUpstream<U>) -> Var<O> {
        let outer = Node::new();
        let inner = node.into_upstream();
        outer.add_edges(inner.node_id());
        let var_cell = Arc::new(VarCell {
            cell: RwLock::new(Arc::new(inner)),
            cache: Cache::new(),
            node: outer,
        });
        Var { node: var_cell }
    }

    pub fn set<U: Upstream<Output = O> + 'static>(&mut self, node: impl IntoUpstream<U>) {
        let mut write = self.node.cell.write();
        self.node.node.remove_edges(write.node_id());
        let new_node = Arc::new(node.into_upstream());
        self.node.node.add_edges(new_node.node_id());
        self.node.cache.invalidate();
        *write = new_node;
    }

    pub fn freeze(&self) -> Arc<dyn Upstream<Output = O>> {
        (*self.node.cell.read()).clone()
    }
}

impl<O: Send> Upstream for VarCell<O> {
    type Output = O;

    fn node_id(&self) -> Option<super::NodeId> {
        Some(self.node.id)
    }
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

impl<O: Send> IntoUpstream<Arc<VarCell<O>>> for Var<O> {
    fn into_upstream(self) -> Arc<VarCell<O>> {
        self.node
    }
}

impl<O: Send> IntoUpstream<Arc<VarCell<O>>> for &Var<O> {
    fn into_upstream(self) -> Arc<VarCell<O>> {
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
