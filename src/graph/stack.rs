use std::sync::Arc;

use parking_lot::RwLock;

use crate::{Callback, Ctx, Data, Upstream, cache::Cache, graph::NodeTracker, node::Node};

use super::NodeId;

pub struct Stack<'a, O> {
    node: NodeTracker,
    nodes: RwLock<Vec<Arc<dyn Upstream<Output = O> + 'a>>>,
    cache: Cache<()>,
}

impl<'a, O: Data> Stack<'a, O> {
    pub fn new(run: impl Upstream<Output = O> + 'a) -> Stack<'a, O> {
        let node = NodeTracker::new();
        node.add_edges(run.node_id());
        Stack {
            nodes: RwLock::new(vec![Arc::new(run)]),
            cache: Cache::new(),
            node,
        }
    }

    pub fn push<U: Upstream<Output = O> + 'a>(
        &self,
        f: impl FnOnce(Arc<dyn Upstream<Output = O> + 'a>) -> U,
    ) {
        let mut nodes = self.nodes.write();
        let prev = nodes.last().unwrap().clone();
        self.node.remove_edges(prev.node_id());
        let upstream = f(prev);
        self.node.add_edges(upstream.node_id());
        nodes.push(Arc::new(upstream));
        self.cache.invalidate()
    }

    pub fn pop(&self) -> Option<Node<Arc<dyn Upstream<Output = O> + 'a>>>
    where
        O: 'static,
    {
        let mut nodes = self.nodes.write();
        if nodes.len() > 1 {
            self.cache.invalidate();
            let node = nodes.pop().unwrap();
            self.node.remove_edges(node.node_id());
            self.node.add_edges(nodes.last().unwrap().node_id());
            Some(Node(node))
        } else {
            None
        }
    }

    pub fn freeze(&self) -> Node<Arc<dyn Upstream<Output = O> + 'a>>
    where
        O: 'static,
    {
        Node(self.nodes.read().last().unwrap().clone())
    }

    pub fn fork(&self) -> Stack<'a, O>
    where
        O: Send + Clone + 'static,
    {
        let node = NodeTracker::new();
        let vec = self.nodes.read();

        node.add_edges(vec.iter().filter_map(|n| n.node_id()));

        Stack {
            nodes: RwLock::new(vec.clone()),
            cache: Cache::new(),
            node,
        }
    }
}

impl<O: Data> Upstream for Stack<'_, O> {
    type Output = O;

    fn node_id(&self) -> Option<NodeId> {
        Some(self.node.id)
    }
    fn request<'s>(&self, ctx: Ctx<'_, 's>, callback: Callback<'s, O>)
    where
        Self: 's,
    {
        let sender = self.cache.get_invalidator_sender();
        let callback = callback.map(|mut c| {
            c.push_sender(sender, false);
            c
        });
        let cell = self.nodes.read();
        cell.last().unwrap().request(ctx, callback);
    }
}

impl<O: Upstream<Output = O> + Default + 'static> Default for Stack<'_, O> {
    fn default() -> Self {
        Stack {
            node: NodeTracker::new(),
            nodes: RwLock::new(vec![Arc::new(O::default()) as Arc<dyn Upstream<Output = O>>]),
            cache: Cache::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as desparrow;
    use crate::graph::variable::Var;
    use crate::*;

    #[test]
    fn test_stack_push_pop() {
        let stack = Stack::new(0);
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
        let stack = Stack::new(0);
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
        let var = Var::new(2);

        let stack = Stack::new(2);
        stack.push(|p| op!(i!(p) * i!(&var)));
        stack.push(|p| op!(i!(p) + 10));

        assert_eq!(run(&stack), 14);

        var.set(5);
        assert_eq!(run(&stack), 20);
    }
}
