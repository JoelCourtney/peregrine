use std::sync::Arc;

use parking_lot::RwLock;

use crate::{Callback, Ctx, Data, Upstream, cache::Cache, node::Node};

pub struct Stack<'a, O> {
    base: Arc<dyn Upstream<Output = O> + 'a>,
    nodes: RwLock<Vec<Arc<dyn Upstream<Output = O> + 'a>>>,
    cache: Cache<()>,
}

impl<'a, O: Data> Stack<'a, O> {
    pub fn new(run: impl Upstream<Output = O> + 'a) -> Stack<'a, O> {
        Stack {
            base: Arc::new(run),
            nodes: RwLock::new(vec![]),
            cache: Cache::new(),
        }
    }

    pub fn push<U: Upstream<Output = O> + 'a>(
        &self,
        f: impl FnOnce(Arc<dyn Upstream<Output = O> + 'a>) -> U,
    ) {
        let mut nodes = self.nodes.write();
        let prev = nodes.last().unwrap_or(&self.base).clone();
        let upstream = f(prev);
        nodes.push(Arc::new(upstream));
        self.cache.invalidate();
    }

    pub fn pop(&self) -> Option<Node<Arc<dyn Upstream<Output = O> + 'a>>>
    where
        O: 'static,
    {
        let mut nodes = self.nodes.write();
        if let Some(node) = nodes.pop() {
            self.cache.invalidate();
            Some(Node(node))
        } else {
            None
        }
    }
}

impl<O: Data> Upstream for Stack<'_, O> {
    type Output = O;

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
        cell.last().unwrap_or(&self.base).request(ctx, callback);
    }
}

impl<O: Upstream<Output = O> + Default + 'static> Default for Stack<'_, O> {
    fn default() -> Self {
        Stack {
            base: Arc::new(O::default()),
            nodes: RwLock::new(vec![]),
            cache: Cache::new(),
        }
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
