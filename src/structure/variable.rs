use forte::Worker;
use std::cell::Cell;

use crate::{
    IntoNode, Node,
    cache::{Cache, MaybeCached},
    env::Env,
};

/// A container for a node that can be overwritten.
///
/// Downstream caches are invalidated when either contained
/// node's cache is invalidated or when the node is overwritten.
pub struct Var<'e, O: 'e> {
    env: Env<'e>,
    node: &'e VarNode<'e, O>,
}

pub struct VarNode<'e, O: 'e> {
    cell: Cell<&'e dyn Node<Output = O>>,
    cache: Cache<()>,
}

impl<O> Var<'_, O> {
    pub fn new<N: Node<Output = O> + 'static>(node: impl IntoNode<N>) -> Var<'static, O> {
        let env = Env::new();
        Var {
            node: env.alloc(VarNode {
                cell: Cell::new(env.alloc(node.into_node()) as &dyn Node<Output = O>),
                cache: Cache::new(),
            }),
            env,
        }
    }

    pub fn set<N: Node<Output = O> + 'static>(&mut self, node: impl IntoNode<N>) {
        self.node.cell.set(self.env.alloc(node.into_node()));
        self.node.cache.invalidate();
    }

    pub fn freeze(&self) -> &dyn Node<Output = O> {
        self.node.cell.get()
    }
}

impl<O: Clone + Send> Node for VarNode<'_, O> {
    type Output = O;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        let mut result = self.cell.get().run(w);
        result.push_sender(self.cache.get_invalidation_sender());
        result
    }
}

unsafe impl<O> Send for VarNode<'_, O> {}
unsafe impl<O> Sync for VarNode<'_, O> {}

impl<'e, O: Send + Clone> IntoNode<&'e VarNode<'e, O>> for Var<'e, O> {
    fn into_node(self) -> &'e VarNode<'e, O> {
        self.node
    }
}

impl<'e, O: Send + Clone> IntoNode<&'e VarNode<'e, O>> for &Var<'e, O> {
    fn into_node(self) -> &'e VarNode<'e, O> {
        self.node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as peregrine;
    use crate::structure::stack::Stack;
    use crate::*;

    #[test]
    fn var() {
        let mut var = Var::new(0);
        assert_eq!(run(&var), 0);

        var.set(stack! {
            v = 2,
            op! {
                let result = v * 3;
                result + 1
            }
        });

        assert_eq!(run(&var), 7);
    }
}
