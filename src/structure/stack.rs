use std::sync::Arc;

use forte::Worker;
use parking_lot::Mutex;

use crate::{cache::{Cache, MaybeCached}, node::NodeArc, IntoNode, Node};

pub struct Stack<O>(Mutex<Vec<NodeArc<O>>>, Arc<Cache<O>>);

impl<O> Stack<O> {
    pub fn new<N: Node<Output = O> + 'static>(node: impl IntoNode<N>) -> Self {
        Stack(
            Mutex::new(vec![NodeArc::new(node.into_node())]),
            Cache::new(),
        )
    }

    pub fn push<N: Node<Output = O> + 'static, IN: IntoNode<N>>(
        &self,
        f: impl FnOnce(NodeArc<O>) -> IN,
    ) {
        let mut nodes = self.0.lock();
        let prev = nodes.last().unwrap().clone();
        nodes.push(NodeArc::new(f(prev).into_node()));
        self.1.invalidate()
    }

    pub fn pop(&self) -> Option<NodeArc<O>> {
        let mut nodes = self.0.lock();
        if nodes.len() > 1 {
            self.1.invalidate();
            nodes.pop()
        } else {
            None
        }
    }

    pub fn freeze(&self) -> NodeArc<O> {
        self.0.lock().last().unwrap().clone()
    }

    pub fn fork(&self) -> Self {
        Stack(Mutex::new(self.0.lock().clone()), Cache::new())
    }
}

impl<O: Clone + Send + Sync + 'static> Node for Stack<O> {
    type Output = O;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        let nodes = self.0.lock();
        let last = nodes.last().unwrap();
        self.1.resolve(w, |g| last.run(w).track(g), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::NodeCell;
    use crate as peregrine;
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
        assert!(stack.pop().is_none());
    }

    #[test]
    fn test_stack_as_upstream() {
        let stack = Arc::new(Stack::new(0));
        let node = op! { i!(stack.clone()) * 2 };

        assert_eq!(run(&node), 0);

        stack.push(|prev| op!(i!(prev) + 2));
        assert_eq!(run(&node), 4);

        assert_eq!(run(stack.pop()), Some(2));
        assert_eq!(run(&node), 0);
        assert!(stack.pop().is_none());
    }
    
    #[test]
    fn test_stack_with_cell() {
        let cell = Arc::new(NodeCell::new(2));
        let stack = Stack::new(2);
        stack.push(|prev| op!(i!(prev) * i!(cell.clone())));
        stack.push(|prev| op!(i!(prev) + 10));
        
        assert_eq!(run(&stack), 14);
        
        cell.set(5);
        assert_eq!(run(&stack), 20);
    }
}
