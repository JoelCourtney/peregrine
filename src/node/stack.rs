use std::sync::Arc;

use parking_lot::RwLock;

use crate::{
    Ctx, IntoRun, Run,
    cache::{Cache, MaybeCached},
    node::Node,
};

pub struct Stack<O> {
    vec: Node<StackVec<O>>,
}

pub struct StackVec<O> {
    nodes: RwLock<Vec<Node<dyn Run<Output = O>>>>,
    cache: Arc<Cache<O>>,
}

impl<O: Clone + Send + 'static> Stack<O> {
    pub fn new<R: Run<Output = O> + 'static>(run: impl IntoRun<R>) -> Stack<O> {
        let outer = Node::empty();
        let inner = Node::new_dyn(run.into_run());
        outer.add_edge(&inner);
        Stack {
            vec: outer.init(StackVec {
                nodes: RwLock::new(vec![inner]),
                cache: Cache::new_arc(),
            }),
        }
    }

    pub fn push<R: Run<Output = O> + 'static, IR: IntoRun<R>>(
        &mut self,
        f: impl FnOnce(Node<dyn Run<Output = O>>) -> IR,
    ) {
        let mut nodes = self.vec.nodes.write();
        let prev = nodes.last().unwrap().clone();
        let ir = f(prev).into_run();
        let new_node = Node::new_dyn(ir);
        self.vec.add_edge(&new_node);
        nodes.push(new_node);
        self.vec.cache.invalidate()
    }

    pub fn pop(&self) -> Option<Node<dyn Run<Output = O>>>
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

    pub fn freeze(&self) -> Node<dyn Run<Output = O>>
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

impl<O: Clone + Send + 'static> Run for StackVec<O> {
    type Output = O;

    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        let nodes = self.nodes.read();
        let last = nodes.last().unwrap();
        self.cache
            .resolve(ctx.worker, |g| last.run(ctx).track(g), true)
    }
}

impl<O: Send + Clone + 'static> IntoRun<Node<StackVec<O>>> for Stack<O> {
    fn into_run(self) -> Node<StackVec<O>> {
        self.vec
    }
}

impl<O: Send + Clone + 'static> IntoRun<Node<StackVec<O>>> for &Stack<O> {
    fn into_run(self) -> Node<StackVec<O>> {
        self.vec.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as peregrine;
    use crate::node::variable::Var;
    use crate::*;

    #[test]
    fn test_stack_push_pop() {
        let mut stack = Stack::new(0);
        assert_eq!(run(&stack), 0);

        stack.push(|prev| op!(prev + 2));
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

        stack.push(|prev| op!(prev + 2));
        assert_eq!(run(&node), 4);

        assert_eq!(run(stack.pop()), Some(2));
        assert_eq!(run(&node), 0);
        assert_eq!(run(stack.pop()), None);
    }

    #[test]
    fn test_stack_with_cell() {
        let mut var = Var::new(2);

        let mut stack = Stack::new(2);
        stack.push(|p| op!(p * i!(&var)));
        stack.push(|p| op!(p + 10));

        assert_eq!(run(&stack), 14);

        var.set(5);
        assert_eq!(run(&stack), 20);
    }
}
