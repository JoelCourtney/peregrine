use std::sync::Arc;

use forte::Worker;
use parking_lot::RwLock;

use crate::{
    IntoNode, Node,
    cache::{Cache, MaybeCached},
    env::Env,
};

#[macro_export]
macro_rules! stack {
    ($node:expr) => {
        Stack::new($crate::op!($node))
    };

    ($v:ident = $init:expr, $($layers:expr),+) => {
        {
            let stack = Stack::new($crate::op!($init));
            $(
                stack.push(|$v| $layers);
            )*
            stack
        }
    }
}

pub struct Stack<'e, O> {
    env: Env<'e>,
    node: &'e StackNode<'e, O>,
}

pub struct StackNode<'e, O> {
    nodes: RwLock<Vec<&'e dyn Node<Output = O>>>,
    cache: Arc<Cache<O>>,
}

impl<'e, O> Stack<'e, O> {
    pub fn new<N: Node<Output = O> + 'static>(node: impl IntoNode<N>) -> Stack<'static, O> {
        let env = Env::new();
        Stack {
            node: env.alloc(StackNode {
                nodes: RwLock::new(vec![env.alloc(node.into_node())]),
                cache: Cache::new_arc(),
            }),
            env,
        }
    }

    pub fn push<N: Node<Output = O> + 'static, IN: IntoNode<N>>(
        &self,
        f: impl FnOnce(&'e dyn Node<Output = O>) -> IN,
    ) {
        let mut nodes = self.node.nodes.write();
        let prev = *nodes.last().unwrap();
        nodes.push(self.env.alloc(f(prev).into_node()));
        self.node.cache.invalidate()
    }

    pub fn pop(&self) -> Option<&'e dyn Node<Output = O>> {
        let mut nodes = self.node.nodes.write();
        if nodes.len() > 1 {
            self.node.cache.invalidate();
            nodes.pop()
        } else {
            None
        }
    }

    pub fn freeze(&self) -> &'e dyn Node<Output = O> {
        *self.node.nodes.read().last().unwrap()
    }

    pub fn fork(&'e self) -> Stack<'e, O> {
        let env = self.env.borrow();
        Stack {
            node: env.alloc(StackNode {
                nodes: RwLock::new(self.node.nodes.read().clone()),
                cache: Cache::new_arc(),
            }),
            env,
        }
    }
}

impl<'e, O: Clone + Send + 'static> Node for StackNode<'e, O> {
    type Output = O;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        let nodes = self.nodes.read();
        let last = nodes.last().unwrap();
        self.cache.resolve(w, |g| last.run(w).track(g), true)
    }
}

impl<'e, O: Send + Clone + 'static> IntoNode<&'e StackNode<'e, O>> for Stack<'e, O> {
    fn into_node(self) -> &'e StackNode<'e, O> {
        self.node
    }
}

impl<'e, O: Send + Clone + 'static> IntoNode<&'e StackNode<'e, O>> for &Stack<'e, O> {
    fn into_node(self) -> &'e StackNode<'e, O> {
        self.node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as peregrine;
    use crate::structure::variable::Var;
    use crate::*;

    #[test]
    fn test_stack_push_pop() {
        let stack = Stack::new(0);
        assert_eq!(run(&stack), 0);

        stack.push(|prev| op!(prev + 2));
        assert_eq!(run(&stack), 2);

        stack.push(|_| 10);
        assert_eq!(run(&stack), 10);

        assert_eq!(run(stack.pop()), Some(10));
        assert_eq!(run(stack.pop()), Some(2));
        assert!(stack.pop().is_none());
    }

    #[test]
    fn test_stack_as_upstream() {
        let stack = Stack::new(0);
        let node = op! { i!(&stack) * 2 };

        assert_eq!(run(&node), 0);

        stack.push(|prev| op!(prev + 2));
        assert_eq!(run(&node), 4);

        assert_eq!(run(stack.pop()), Some(2));
        assert_eq!(run(&node), 0);
        assert!(stack.pop().is_none());
    }

    #[test]
    fn test_stack_with_cell() {
        let mut var = Var::new(2);
        let stack = stack! {
            v = 2,
            op! { v * i!(&var) },
            op! { v + 10 }
        };

        assert_eq!(run(&stack), 14);

        var.set(5);
        assert_eq!(run(&stack), 20);
    }
}
