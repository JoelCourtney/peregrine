use std::{
    cmp::{Ordering, max},
    collections::BinaryHeap,
    mem::transmute,
    sync::Arc,
};

use parking_lot::{Mutex, MutexGuard};
use peregrine_macros::op;

use crate::{
    Callback, Ctx, Upstream,
    cache::{Cache, CheckResult},
    data::Data,
    node::Node,
};

pub trait NodeGenerator<I, O> {
    fn get(&self, input: I, inclusive: bool) -> Arc<dyn Upstream<Output = GeneratedNode<I, O>>>;
}

pub struct GeneratedNode<I, O> {
    pub placement: I,
    pub node: Arc<dyn Upstream<Output = O>>,
}

impl<I: Clone, O> Clone for GeneratedNode<I, O> {
    fn clone(&self) -> Self {
        Self {
            placement: self.placement.clone(),
            node: self.node.clone(),
        }
    }
}

impl<I: PartialEq, O> PartialEq for GeneratedNode<I, O> {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

pub struct SingleNodeGenerator<I, O> {
    node: Arc<dyn Upstream<Output = GeneratedNode<I, O>>>,
}

impl<I: Data, O: 'static> SingleNodeGenerator<I, O> {
    pub fn new<U: Upstream<Output = O> + 'static>(
        delay: impl Upstream<Output = I> + 'static,
        node_fn: impl (Fn(I) -> U) + Send + Sync + 'static,
    ) -> Self {
        use crate as peregrine;

        Self {
            node: Arc::new(op! {
                let placement = i!(delay);
                let node = Arc::new(node_fn(placement.clone()));
                GeneratedNode {
                    placement,
                    node,
                }
            }),
        }
    }
}

impl<I, O> NodeGenerator<I, O> for SingleNodeGenerator<I, O> {
    fn get(&self, _input: I, _inclusive: bool) -> Arc<dyn Upstream<Output = GeneratedNode<I, O>>> {
        self.node.clone()
    }
}

type UpstreamStepper<I, Eq> = dyn Fn(Node<Arc<dyn Upstream<Output = (I, Eq)>>>) -> Box<dyn Upstream<Output = (I, Eq)>>
    + Send
    + Sync
    + 'static;
type UpstreamBuilder<I, O, Eq> =
    dyn Fn(I, Eq) -> Box<dyn Upstream<Output = O>> + Send + Sync + 'static;
type UpstreamCache<I, O, Eq> = Mutex<Vec<UpstreamStep<I, O, Eq>>>;

struct UpstreamStep<I, O, Eq> {
    index: I,
    cache: Arc<Cache<CachedUpstream<O>>>,
    placement_upstream: Arc<dyn Upstream<Output = (I, Eq)>>,
}

struct CachedUpstream<O> {
    upstream: Arc<dyn Upstream<Output = O>>,
}

impl<O> PartialEq for CachedUpstream<O> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl<O> Clone for CachedUpstream<O> {
    fn clone(&self) -> Self {
        Self {
            upstream: self.upstream.clone(),
        }
    }
}

pub struct RecurringNodeGenerator<I, O, Eq = ()> {
    start: (I, Eq),
    step_fn: Box<UpstreamStepper<I, Eq>>,
    node_fn: Box<UpstreamBuilder<I, O, Eq>>,
    nodes: UpstreamCache<I, O, Eq>,
    callbacks: Mutex<BinaryHeap<RevOrderedCallback<I, O>>>,
    state: Mutex<RecurringNodeGenState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RecurringNodeGenState {
    NotRunning,
    Running,
}

impl<I: Data + Ord, O: 'static, Eq: Data + Default> RecurringNodeGenerator<I, O, Eq> {
    pub fn new(
        start_index: I,
        step_fn: Box<UpstreamStepper<I, Eq>>,
        node_fn: Box<UpstreamBuilder<I, O, Eq>>,
    ) -> Self {
        Self {
            start: (start_index, Eq::default()),
            step_fn,
            node_fn,
            nodes: Mutex::new(Vec::new()),
            callbacks: Mutex::new(BinaryHeap::new()),
            state: Mutex::new(RecurringNodeGenState::NotRunning),
        }
    }

    fn request<'s>(
        &self,
        index: I,
        inclusive: bool,
        ctx: crate::Ctx<'_, '_, 's>,
        callback: Callback<'s, GeneratedNode<I, O>>,
    ) {
        {
            let mut callbacks = self.callbacks.lock();
            callbacks.push(RevOrderedCallback {
                index,
                inclusive,
                callback: unsafe { transmute::<Callback<'s, _>, Callback<'static, _>>(callback) },
            });
        }
        let mut state = self.state.lock();
        if *state == RecurringNodeGenState::NotRunning {
            *state = RecurringNodeGenState::Running;
            drop(state);
            let _callbacks = self.run(ctx);
            *self.state.lock() = RecurringNodeGenState::NotRunning;
        }
    }

    fn run<'s>(
        &self,
        ctx: Ctx<'_, '_, 's>,
    ) -> MutexGuard<'_, BinaryHeap<RevOrderedCallback<I, O>>> {
        let send_result = |ctx: &Ctx<'_, '_, 's>,
                           node: &UpstreamStep<I, O, Eq>,
                           callback: Callback<'static, _>| {
            let result = match node.cache.check() {
                CheckResult::NoProblem(v) => v.map(|v| GeneratedNode {
                    placement: node.index.clone(),
                    node: v.upstream,
                }),
                _ => unreachable!(),
            };
            ctx.spawn(move |ctx| callback.call(result, ctx));
        };
        let nodes = self.nodes.lock();
        loop {
            let mut callbacks = self.callbacks.lock();
            let next_callback = callbacks.pop();
            if let Some(c) = next_callback {
                drop(callbacks);
                let search_result = nodes.binary_search_by(|step| {
                    if step.cache.is_valid() {
                        step.index.cmp(&c.index)
                    } else {
                        Ordering::Greater
                    }
                });
                match search_result {
                    Ok(which) if c.inclusive => send_result(&ctx, &nodes[which], c.callback),
                    Ok(which) => send_result(&ctx, &nodes[max(which - 1, 0)], c.callback),
                    Err(which) => {
                        if nodes[which].cache.is_valid() {
                            send_result(&ctx, &nodes[max(which - 1, 0)], c.callback);
                        } else {
                            todo!()
                        }
                    }
                };
            } else {
                return callbacks;
            }
        }
    }
}

struct RevOrderedCallback<I, O> {
    index: I,
    inclusive: bool,
    callback: Callback<'static, GeneratedNode<I, O>>,
}

impl<I: Ord, O> Ord for RevOrderedCallback<I, O> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.index.cmp(&other.index).reverse()
    }
}

impl<I: Ord, O> PartialOrd for RevOrderedCallback<I, O> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<I: Ord, O> Eq for RevOrderedCallback<I, O> {}

impl<I: Ord, O> PartialEq for RevOrderedCallback<I, O> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<I: Ord + Data, O: Data, Eq: Default + Data> NodeGenerator<I, O>
    for Arc<RecurringNodeGenerator<I, O, Eq>>
{
    fn get(&self, input: I, inclusive: bool) -> Arc<dyn Upstream<Output = GeneratedNode<I, O>>> {
        Arc::new(RecurringNodeUpstream {
            generator: self.clone(),
            query: input,
            inclusive,
        })
    }
}

pub struct RecurringNodeUpstream<I, O, Eq = ()> {
    generator: Arc<RecurringNodeGenerator<I, O, Eq>>,
    query: I,
    inclusive: bool,
}

impl<I: Data + Ord, O: Data, Eq: Default + Data> Upstream for RecurringNodeUpstream<I, O, Eq> {
    type Output = GeneratedNode<I, O>;

    fn request<'s>(&self, ctx: crate::Ctx<'_, '_, 's>, callback: Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        self.generator
            .request(self.query.clone(), self.inclusive, ctx, callback);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        self as peregrine,
        graph::{series::resource::Resource, variable::Var},
        plan::Time,
        run,
    };

    #[test]
    fn test_delayed_node() {
        let delay = Arc::new(Var::new(Time::from_tai_seconds(1.0)));

        let x = Arc::new(Resource::new(0));
        let id = x.set_at(Time::from_tai_seconds(2.0), 5);

        let x_clone = x.clone();
        let delayed_node = SingleNodeGenerator::new(delay.clone(), move |t| {
            op! {
                let x = i!(x_clone.get_at(t));
                x + 1
            }
        });

        let higher_order_node = delayed_node.get(Time::default(), false);
        let generated_node = run(&higher_order_node);

        assert_eq!(generated_node.placement, Time::from_tai_seconds(1.0));
        assert_eq!(run(generated_node.node), 1);

        delay.set(Time::from_tai_seconds(5.0));

        let generated_node = run(&higher_order_node);
        assert_eq!(generated_node.placement, Time::from_tai_seconds(5.0));
        assert_eq!(run(&generated_node.node), 6);

        x.remove(id);

        assert_eq!(run(generated_node.node), 1);
        let generated_node = run(&higher_order_node);
        assert_eq!(generated_node.placement, Time::from_tai_seconds(5.0));
        assert_eq!(run(&generated_node.node), 1);
    }
}
