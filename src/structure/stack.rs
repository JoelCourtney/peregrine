use std::sync::Arc;

use forte::Worker;
use parking_lot::RwLock;

use crate::{
    IntoNode, Node,
    cache::{Cache, MaybeCached},
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

pub struct Stack<O>(RwLock<Vec<Arc<dyn Node<Output=O>>>>, Arc<Cache<O>>);

impl<O> Stack<O> {
    pub fn new<N: Node<Output = O> + 'static>(node: impl IntoNode<N>) -> Self {
        Stack(
            RwLock::new(vec![Arc::new(node.into_node())]),
            Cache::new_arc(),
        )
    }

    pub fn push<N: Node<Output = O> + 'static, IN: IntoNode<N>>(
        &self,
        f: impl FnOnce(Arc<dyn Node<Output=O>>) -> IN,
    ) {
        let mut nodes = self.0.write();
        let prev = nodes.last().unwrap().clone();
        nodes.push(Arc::new(f(prev).into_node()));
        self.1.invalidate()
    }

    pub fn pop(&self) -> Option<Arc<dyn Node<Output=O>>> {
        let mut nodes = self.0.write();
        if nodes.len() > 1 {
            self.1.invalidate();
            nodes.pop()
        } else {
            None
        }
    }

    pub fn freeze(&self) -> Arc<dyn Node<Output=O>> {
        self.0.read().last().unwrap().clone()
    }

    pub fn fork(&self) -> Self {
        Stack(RwLock::new(self.0.read().clone()), Cache::new_arc())
    }
}

impl<O: Clone + Send + Sync + 'static> Node for Stack<O> {
    type Output = O;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        let nodes = self.0.read();
        let last = nodes.last().unwrap();
        self.1.resolve(w, |g| last.run(w).track(g), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as peregrine;
    use crate::structure::NodeCell;
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
        let stack = Arc::new(Stack::new(0));
        let node = op! { i!(stack.clone()) * 2 };

        assert_eq!(run(&node), 0);

        stack.push(|prev| op!(prev + 2));
        assert_eq!(run(&node), 4);

        assert_eq!(run(stack.pop()), Some(2));
        assert_eq!(run(&node), 0);
        assert!(stack.pop().is_none());
    }

    #[test]
    fn test_stack_with_cell() {
        let cell = Arc::new(NodeCell::new(2));
        let stack = stack! {
            v = 2,
            op! { v * i!(cell.clone()) },
            op! { v + 10 }
        };

        assert_eq!(run(&stack), 14);

        cell.set(5);
        assert_eq!(run(&stack), 20);
    }
}
