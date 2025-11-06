use std::sync::Arc;

use crate::{Callback, Ctx, IntoUpstream, Upstream, cache::Cached, data::Data};

impl<U: Upstream> IntoUpstream<U> for U {
    fn into_upstream(self) -> Self {
        self
    }
}

impl<U: Upstream + ?Sized> Upstream for &U {
    type Output = U::Output;

    fn request(&self, ctx: Ctx, callback: Callback<Self::Output>) {
        (**self).request(ctx, callback)
    }
}

impl<U: Upstream + ?Sized> Upstream for Box<U> {
    type Output = U::Output;

    fn request(&self, ctx: Ctx, callback: Callback<Self::Output>) {
        (**self).request(ctx, callback)
    }
}

impl<U: Upstream + ?Sized> Upstream for Arc<U> {
    type Output = U::Output;

    fn request(&self, ctx: Ctx, callback: Callback<Self::Output>) {
        (**self).request(ctx, callback)
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct DataWrapper<O>(O);

impl<O: Data> Upstream for DataWrapper<O> {
    type Output = O;

    #[inline(always)]
    fn request(&self, ctx: Ctx, callback: Callback<Self::Output>) {
        callback.call(Cached::Constant(self.0.clone()), ctx);
    }
}

impl<O: Data> IntoUpstream<DataWrapper<O>> for O {
    fn into_upstream(self) -> DataWrapper<O> {
        DataWrapper(self)
    }
}

#[derive(Debug)]
pub struct FnWrapper<F>(F);

impl<O: Send + 'static, F: Fn() -> Cached<O> + Send + Sync> Upstream for FnWrapper<F> {
    type Output = O;

    #[inline(always)]
    fn request<'s>(&self, ctx: Ctx<'_, 's>, callback: Callback<Self::Output>) {
        callback.call(self.0(), ctx);
    }
}

impl<O: Send + 'static, F: Fn() -> Cached<O> + Send + Sync> IntoUpstream<FnWrapper<F>> for F {
    fn into_upstream(self) -> FnWrapper<F> {
        FnWrapper(self)
    }
}

impl<R: Upstream> Upstream for Option<R> {
    type Output = Option<R::Output>;

    fn request(&self, ctx: Ctx, callback: Callback<Option<R::Output>>) {
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

impl<U: Upstream, O: Send + 'static> Upstream for UncachedMap<U, O> {
    type Output = O;

    fn request(&self, ctx: Ctx, callback: Callback<Self::Output>) {
        let func = self.func;
        self.upstream
            .request(ctx, callback.map(move |c| c.map(func)))
    }
}
