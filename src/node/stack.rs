use std::sync::Arc;

use parking_lot::RwLock;

use crate::{
    Ctx, IntoRun, Run,
    cache::{Cache, MaybeCached},
    node::Node,
    world::{World, WorldId},
};

pub struct Stack<'e, O> {
    world: &'e World,
    vec: Node<StackVec<O>>,
}

pub struct StackVec<O> {
    nodes: RwLock<Vec<Node<dyn Run<Output = O>>>>,
    cache: Arc<Cache<O>>,
}

impl<'e, O: Clone + Send + 'static> Stack<'e, O> {
    pub fn new<R: Run<Output = O> + 'static>(world: &World, run: impl IntoRun<R>) -> Stack<'_, O> {
        Stack {
            vec: world.alloc(StackVec {
                nodes: RwLock::new(vec![world.alloc(run.into_run()).as_dyn()]),
                cache: Cache::new_arc(),
            }),
            world,
        }
    }

    pub fn push<R: Run<Output = O> + 'static, IR: IntoRun<R>>(
        &mut self,
        f: impl FnOnce(Node<dyn Run<Output = O>>) -> IR,
    ) {
        let v = self.world.get(self.vec);
        let mut nodes = v.nodes.write();
        let prev = *nodes.last().unwrap();
        nodes.push(self.world.alloc(f(prev).into_run()).as_dyn());
        v.cache.invalidate()
    }

    pub fn pop(&self) -> Option<Node<dyn Run<Output = O>>>
    where
        O: 'static,
    {
        let v = self.world.get(self.vec);
        let mut nodes = v.nodes.write();
        if nodes.len() > 1 {
            v.cache.invalidate();
            nodes.pop()
        } else {
            None
        }
    }

    pub fn freeze(&self) -> Node<dyn Run<Output = O>>
    where
        O: 'static,
    {
        let v = self.world.get(self.vec);
        *v.nodes.read().last().unwrap()
    }

    pub fn fork(&self) -> Stack<'e, O>
    where
        O: Send + Clone + 'static,
    {
        let v = self.world.get(self.vec);
        Stack {
            vec: self.world.alloc(StackVec {
                nodes: RwLock::new(v.nodes.read().clone()),
                cache: Cache::new_arc(),
            }),
            world: self.world,
        }
    }
}

impl<O: Clone + Send + 'static> Run for StackVec<O> {
    type Output = O;

    fn world_id(&self) -> WorldId {
        self.nodes.read().first().unwrap().world_id
    }
    fn run(&self, ctx: Ctx) -> MaybeCached<Self::Output> {
        let nodes = self.nodes.read();
        let last = nodes.last().unwrap();
        self.cache
            .resolve(ctx.worker, |g| last.run(ctx).track(g), true)
    }
}

impl<'e, O: Send + Clone + 'static> IntoRun<Node<dyn Run<Output = O>>> for Stack<'e, O> {
    fn into_run(self) -> Node<dyn Run<Output = O>> {
        self.freeze()
    }
}

impl<'e, O: Send + Clone + 'static> IntoRun<Node<StackVec<O>>> for &Stack<'e, O> {
    fn into_run(self) -> Node<StackVec<O>> {
        self.vec
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
        let w = World::new();

        let mut stack = Stack::new(&w, 0);
        assert_eq!(run(&w, &stack), Ok(0));

        stack.push(|prev| op!(prev + 2));
        assert_eq!(run(&w, &stack), Ok(2));

        stack.push(|_| 10);
        assert_eq!(run(&w, &stack), Ok(10));

        assert_eq!(run(&w, stack.pop()), Ok(Some(10)));
        assert_eq!(run(&w, stack.pop()), Ok(Some(2)));
        assert!(stack.pop().is_none());
    }

    #[test]
    fn test_stack_as_upstream() {
        let w = World::new();

        let mut stack = Stack::new(&w, 0);
        let node = op! { i!(&stack) * 2 };

        assert_eq!(run(&w, &node), Ok(0));

        stack.push(|prev| op!(prev + 2));
        assert_eq!(run(&w, &node), Ok(4));

        assert_eq!(run(&w, stack.pop()), Ok(Some(2)));
        assert_eq!(run(&w, &node), Ok(0));
        assert!(stack.pop().is_none());
    }

    #[test]
    fn test_stack_with_cell() {
        let w = World::new();

        let mut var = Var::new(&w, 2);

        let mut stack = Stack::new(&w, 2);
        stack.push(|p| op!(p * i!(&var)));
        stack.push(|p| op!(p + 10));

        assert_eq!(run(&w, &stack), Ok(14));

        var.set(5);
        assert_eq!(run(&w, &stack), Ok(20));
    }
}
