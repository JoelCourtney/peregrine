use std::sync::Arc;

use parking_lot::RwLock;

use crate::{
    Ctx, IntoRun, Run, RunInWorld,
    cache::{Cache, MaybeCached},
    node::Node,
    world::World,
};

pub struct Stack<O> {
    world: World,
    vec: Node<StackVec<O>>,
}

pub struct StackVec<O> {
    nodes: RwLock<Vec<Node<dyn Run<Output = O>>>>,
    cache: Arc<Cache<O>>,
}

impl<O: Clone + Send + 'static> Stack<O> {
    pub fn new<R: Run<Output = O> + 'static>(run: impl IntoRun<R>) -> Stack<O> {
        let RunInWorld { run, world } = run.into_run();
        Stack {
            vec: world.alloc(StackVec {
                nodes: RwLock::new(vec![world.alloc(run).as_dyn()]),
                cache: Cache::new_arc(),
            }),
            world,
        }
        
    }
    pub fn new_in<R: Run<Output = O> + 'static>(world: World, run: impl IntoRun<R>) -> Stack<O> {
        let RunInWorld { run, world: other_world } = run.into_run();
        world.merge_in_place(other_world).expect("World provided doesn't match the world required by the provide node.");
        Self::new(RunInWorld { run, world })
    }

    pub fn push<R: Run<Output = O> + 'static, IR: IntoRun<R>>(
        &mut self,
        f: impl FnOnce(RunInWorld<Node<dyn Run<Output = O>>>) -> IR,
    ) {
        let v = self.world.get(self.vec);
        let mut nodes = v.nodes.write();
        let prev = *nodes.last().unwrap();
        let ir = f(RunInWorld::new(prev, self.world.clone())).into_run();
        self.world
            .merge_in_place(ir.world)
            .expect("Cannot push node onto stack that requires a different world");
        nodes.push(self.world.alloc(ir.run).as_dyn());
        v.cache.invalidate()
    }

    pub fn pop(&self) -> RunInWorld<Option<Node<dyn Run<Output = O>>>>
    where
        O: 'static,
    {
        let v = self.world.get(self.vec);
        let mut nodes = v.nodes.write();
        let result = if nodes.len() > 1 {
            v.cache.invalidate();
            nodes.pop()
        } else {
            None
        };
        RunInWorld {
            run: result,
            world: self.world.clone(),
        }
    }

    pub fn freeze(&self) -> RunInWorld<Node<dyn Run<Output = O>>>
    where
        O: 'static,
    {
        let v = self.world.get(self.vec);
        RunInWorld {
            run: *v.nodes.read().last().unwrap(),
            world: self.world.clone(),
        }
    }

    pub fn fork(&self) -> Stack<O>
    where
        O: Send + Clone + 'static,
    {
        let v = self.world.get(self.vec);
        Stack {
            vec: self.world.alloc(StackVec {
                nodes: RwLock::new(v.nodes.read().clone()),
                cache: Cache::new_arc(),
            }),
            world: self.world.clone(),
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

impl<O: Send + Clone + 'static> IntoRun<Node<dyn Run<Output = O>>> for Stack<O> {
    fn into_run(self) -> RunInWorld<Node<dyn Run<Output = O>>> {
        let r = *self.world.get(self.vec).nodes.read().last().unwrap();
        RunInWorld {
            run: r,
            world: self.world,
        }
    }
}

impl<O: Send + Clone + 'static> IntoRun<Node<StackVec<O>>> for &Stack<O> {
    fn into_run(self) -> RunInWorld<Node<StackVec<O>>> {
        RunInWorld::new(self.vec, self.world.clone())
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
        assert_eq!(run(&stack), Ok(0));

        stack.push(|prev| op!(prev + 2));
        assert_eq!(run(&stack), Ok(2));

        stack.push(|_| 10);
        assert_eq!(run(&stack), Ok(10));

        assert_eq!(run(stack.pop()), Ok(Some(10)));
        assert_eq!(run(stack.pop()), Ok(Some(2)));
        assert_eq!(run(stack.pop()), Ok(None));
    }

    #[test]
    fn test_stack_as_upstream() {
        let mut stack = Stack::new(0);
        let node = op! { i!(&stack) * 2 };

        assert_eq!(run(&node), Ok(0));

        stack.push(|prev| op!(prev + 2));
        assert_eq!(run(&node), Ok(4));

        assert_eq!(run(stack.pop()), Ok(Some(2)));
        assert_eq!(run(&node), Ok(0));
        assert_eq!(run(stack.pop()), Ok(None));
    }

    #[test]
    fn test_stack_with_cell() {
        let mut var = Var::new(2);

        let mut stack = Stack::new_in(var.world(), 2);
        stack.push(|p| op!(p * i!(&var)));
        stack.push(|p| op!(p + 10));

        assert_eq!(run(&stack), Ok(14));

        var.set(5);
        assert_eq!(run(&stack), Ok(20));
    }
}
