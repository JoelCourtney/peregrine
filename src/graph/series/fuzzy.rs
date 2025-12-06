use std::sync::Arc;

use crate::{
    Callback, Data, Upstream,
    cache::Cached,
    graph::{NodeId, op::Op},
};

use super::dense::DenseSeries;

pub struct FuzzySeries<'a, T: Data, O> {
    series: DenseSeries<'a, T, UpstreamResolver<'a, T, O>>,
}

struct UpstreamResolver<'a, T: Data, O> {
    choices: Vec<Arc<dyn Upstream<Output = O> + 'a>>,
    timing: Op<Vec<Grounding<'a, T>>, usize, Box<dyn Fn(Vec<T>) -> usize>>,
}

pub enum Grounding<'a, T> {
    Static(T),
    Dynamic(Arc<dyn Upstream<Output = T> + 'a>),
}

impl<'a, T: Data> Upstream for Grounding<'a, T> {
    type Output = T;

    #[inline]
    fn node_id(&self) -> Option<NodeId> {
        match self {
            Grounding::Static(_) => None,
            Grounding::Dynamic(upstream) => upstream.node_id(),
        }
    }

    #[inline]
    fn request<'s>(&self, ctx: crate::Ctx<'_, 's>, callback: Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        match self {
            Grounding::Static(t) => callback.call(Cached::Constant(t.clone()), ctx),
            Grounding::Dynamic(upstream) => upstream.request(ctx, callback),
        }
    }
}
