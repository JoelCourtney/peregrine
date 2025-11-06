use std::sync::Arc;

use parking_lot::RwLock;

use crate::{Callback, Ctx, IntoUpstream, Upstream, cache::Cache, graph::Node};

use super::NodeId;

pub struct Stack<O> {
    vec: Arc<StackVec<O>>,
}

pub struct StackVec<O> {
    node: Node,
    nodes: RwLock<Vec<Arc<dyn Upstream<Output = O>>>>,
    cache: Arc<Cache<O>>,
}

impl<O: Clone + Send + 'static> Stack<O> {
    pub fn new<U: Upstream<Output = O> + 'static>(run: impl IntoUpstream<U>) -> Stack<O> {
        let node = Node::new();
        let inner = run.into_upstream();
        node.add_edges(inner.node_id());
        Stack {
            vec: Arc::new(StackVec {
                nodes: RwLock::new(vec![Arc::new(inner)]),
                cache: Cache::new_arc(),
                node,
            }),
        }
    }

    pub fn push<U: Upstream<Output = O> + 'static, IR: IntoUpstream<U>>(
        &mut self,
        f: impl FnOnce(Arc<dyn Upstream<Output = O>>) -> IR,
    ) {
        let mut nodes = self.vec.nodes.write();
        let prev = nodes.last().unwrap().clone();
        self.vec.node.remove_edges(prev.node_id());
        let ir = f(prev).into_upstream();
        self.vec.node.add_edges(ir.node_id());
        nodes.push(Arc::new(ir));
        self.vec.cache.invalidate()
    }

    pub fn pop(&self) -> Option<Arc<dyn Upstream<Output = O>>>
    where
        O: 'static,
    {
        let mut nodes = self.vec.nodes.write();
        if nodes.len() > 1 {
            self.vec.cache.invalidate();
            let node = nodes.pop().unwrap();
            self.vec.node.remove_edges(node.node_id());
            self.vec.node.add_edges(nodes.last().unwrap().node_id());
            Some(node)
        } else {
            None
        }
    }

    pub fn freeze(&self) -> Arc<dyn Upstream<Output = O>>
    where
        O: 'static,
    {
        self.vec.nodes.read().last().unwrap().clone()
    }

    pub fn fork(&self) -> Stack<O>
    where
        O: Send + Clone + 'static,
    {
        let node = Node::new();
        let vec = self.vec.nodes.read();

        node.add_edges(vec.iter().filter_map(|n| n.node_id()));

        Stack {
            vec: Arc::new(StackVec {
                nodes: RwLock::new(vec.clone()),
                cache: Cache::new_arc(),
                node,
            }),
        }
    }
}

impl<O: Send + 'static> Upstream for StackVec<O> {
    type Output = O;

    fn node_id(&self) -> Option<NodeId> {
        Some(self.node.id)
    }
    fn request(&self, ctx: Ctx, callback: Callback<O>) {
        let sender = self.cache.get_invalidator_sender();
        let callback = callback.map(|mut c| {
            c.push_sender(sender, false);
            c
        });
        let cell = self.nodes.read();
        cell.last().unwrap().request(ctx, callback);
    }
}

impl<O: Send + Clone + 'static> IntoUpstream<Arc<StackVec<O>>> for Stack<O> {
    fn into_upstream(self) -> Arc<StackVec<O>> {
        self.vec
    }
}

impl<O: Send + Clone + 'static> IntoUpstream<Arc<StackVec<O>>> for &Stack<O> {
    fn into_upstream(self) -> Arc<StackVec<O>> {
        self.vec.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as peregrine;
    use crate::graph::variable::Var;
    use crate::*;

    #[test]
    fn test_stack_push_pop() {
        let mut stack = Stack::new(0);
        assert_eq!(run(&stack), 0);

        stack.push(|prev| op!(i!(prev) + 2));
        assert_eq!(run(&stack), 2);

        stack.push(|_| 10);
        assert_eq!(run(&stack), 10);

        assert_eq!(run(stack.pop()), Some(10));
        assert_eq!(run(stack.pop()), Some(2));
        assert_eq!(run(stack.pop()), None);
    }

    #[test]
    fn test_stack_as_upstream() {
        let mut stack = Stack::new(0);
        let node = op! { i!(&stack) * 2 };

        assert_eq!(run(&node), 0);

        stack.push(|prev| op!(i!(prev) + 2));
        assert_eq!(run(&node), 4);

        assert_eq!(run(stack.pop()), Some(2));
        assert_eq!(run(&node), 0);
        assert_eq!(run(stack.pop()), None);
    }

    #[test]
    fn test_stack_with_cell() {
        let mut var = Var::new(2);

        let mut stack = Stack::new(2);
        stack.push(|p| op!(i!(p) * i!(&var)));
        stack.push(|p| op!(i!(p) + 10));

        assert_eq!(run(&stack), 14);

        var.set(5);
        assert_eq!(run(&stack), 20);
    }
}
