use std::sync::Arc;

use crate::{Callback, Ctx, Data, Upstream, cache::Cached};

use super::NodeId;

impl<U: Upstream + ?Sized> Upstream for &U {
    type Output = U::Output;

    fn node_id(&self) -> Option<NodeId> {
        (**self).node_id()
    }
    fn request<'s>(&self, ctx: Ctx<'_, 's>, callback: Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        (**self).request(ctx, callback)
    }
}

impl<U: Upstream + ?Sized> Upstream for Box<U> {
    type Output = U::Output;

    fn node_id(&self) -> Option<NodeId> {
        (**self).node_id()
    }
    fn request<'s>(&self, ctx: Ctx<'_, 's>, callback: Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        (**self).request(ctx, callback)
    }
}

impl<U: Upstream + ?Sized> Upstream for Arc<U> {
    type Output = U::Output;

    fn node_id(&self) -> Option<NodeId> {
        (**self).node_id()
    }
    fn request<'s>(&self, ctx: Ctx<'_, 's>, callback: Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        (**self).request(ctx, callback)
    }
}

impl<R: Upstream> Upstream for Option<R> {
    type Output = Option<R::Output>;

    fn node_id(&self) -> Option<NodeId> {
        self.as_ref().and_then(|this| this.node_id())
    }
    fn request<'s>(&self, ctx: Ctx<'_, 's>, callback: Callback<'s, Option<R::Output>>)
    where
        Self: 's,
    {
        match self {
            Some(r) => r.request(ctx, callback.map(|o| o.map(Some))),
            None => callback.call(Cached::Constant(None), ctx),
        }
    }
}

pub struct UncachedMap<U: Upstream, O> {
    pub(crate) upstream: U,
    pub(crate) func: fn(U::Output) -> O,
}

impl<U: Upstream, O: Data> Upstream for UncachedMap<U, O> {
    type Output = O;

    fn node_id(&self) -> Option<NodeId> {
        self.upstream.node_id()
    }
    fn request<'s>(&self, ctx: Ctx<'_, 's>, callback: Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        let func = self.func;
        self.upstream
            .request(ctx, callback.map(move |c| c.map(func)))
    }
}
