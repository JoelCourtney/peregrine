use std::sync::Arc;

use parking_lot::RwLock;

use crate::{Callback, Ctx, IntoUpstream, Upstream, cache::Cache, graph::Node};

pub struct Stack<O> {
    vec: Node<StackVec<O>>,
}

pub struct StackVec<O> {
    nodes: RwLock<Vec<Node<dyn Upstream<Output = O>>>>,
    cache: Arc<Cache<O>>,
}

impl<O: Clone + Send + 'static> Stack<O> {
    pub fn new<U: Upstream<Output = O> + 'static>(run: impl IntoUpstream<U>) -> Stack<O> {
        let outer = Node::empty();
        let inner = Node::new_dyn(run.into_upstream());
        outer.add_edge(&inner);
        Stack {
            vec: outer.init(StackVec {
                nodes: RwLock::new(vec![inner]),
                cache: Cache::new_arc(),
            }),
        }
    }

    pub fn push<U: Upstream<Output = O> + 'static, IR: IntoUpstream<U>>(
        &mut self,
        f: impl FnOnce(Node<dyn Upstream<Output = O>>) -> IR,
    ) {
        let mut nodes = self.vec.nodes.write();
        let prev = nodes.last().unwrap().clone();
        let ir = f(prev).into_upstream();
        let new_node = Node::new_dyn(ir);
        self.vec.add_edge(&new_node);
        nodes.push(new_node);
        self.vec.cache.invalidate()
    }

    pub fn pop(&self) -> Option<Node<dyn Upstream<Output = O>>>
    where
        O: 'static,
    {
        let mut nodes = self.vec.nodes.write();
        if nodes.len() > 1 {
            self.vec.cache.invalidate();
            let node = nodes.pop();
            self.vec.remove_edge(node.as_ref().unwrap());
            node
        } else {
            None
        }
    }

    pub fn freeze(&self) -> Node<dyn Upstream<Output = O>>
    where
        O: 'static,
    {
        self.vec.nodes.read().last().unwrap().clone()
    }

    pub fn fork(&self) -> Stack<O>
    where
        O: Send + Clone + 'static,
    {
        let new_outer = Node::empty();

        let nodes = self.vec.nodes.read();
        for node in nodes.iter() {
            new_outer.add_edge(node);
        }

        Stack {
            vec: new_outer.init(StackVec {
                nodes: RwLock::new(nodes.clone()),
                cache: Cache::new_arc(),
            }),
        }
    }
}

impl<O: Send + 'static> Upstream for StackVec<O> {
    type Output = O;

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

impl<O: Send + Clone + 'static> IntoUpstream<Node<StackVec<O>>> for Stack<O> {
    fn into_upstream(self) -> Node<StackVec<O>> {
        self.vec
    }
}

impl<O: Send + Clone + 'static> IntoUpstream<Node<StackVec<O>>> for &Stack<O> {
    fn into_upstream(self) -> Node<StackVec<O>> {
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
