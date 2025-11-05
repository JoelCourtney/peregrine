use std::sync::Arc;

use crate::{
    Callback, Ctx, IntoUpstream, Upstream,
    cache::Cached,
    data::Data,
    graph::op::Op,
};
use crossbeam::atomic::AtomicCell;

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

macro_rules! impl_into_upstream_for_tuple {
    ($($t:ident $t_i:ident),*) => {
        impl<$($t: Upstream + 'static, $t_i: IntoUpstream<$t>),*> IntoUpstream<Op<($($t,)*), ($(AtomicCell<Option<Cached<$t::Output>>>,)*), ($($t::Output,)*), fn(($($t::Output,)*)) -> ($($t::Output,)*)>> for ($($t_i,)*)
        where $($t::Output: Send + Clone + 'static, )* {
            #[allow(non_snake_case)]
            fn into_upstream(self) -> Op<($($t,)*), ($(AtomicCell<Option<Cached<$t::Output>>>,)*), ($($t::Output,)*), fn(($($t::Output,)*)) -> ($($t::Output,)*)> {
                let ($($t_i,)*) = self;

                let ($($t_i,)*) = ($($t_i.into_upstream()),*);

                Op::new(($($t_i,)*), identity)
            }
        }
    };
}

fn identity<T>(value: T) -> T {
    value
}

impl_into_upstream_for_tuple!(A AI, B BI);
impl_into_upstream_for_tuple!(A AI, B BI, C CI);
impl_into_upstream_for_tuple!(A AI, B BI, C CI, D DI);
impl_into_upstream_for_tuple!(A AI, B BI, C CI, D DI, E EI);
impl_into_upstream_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI);
impl_into_upstream_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI);
impl_into_upstream_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI);
impl_into_upstream_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI, I II);
impl_into_upstream_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI, I II, J JI);
impl_into_upstream_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI, I II, J JI, K KI);
impl_into_upstream_for_tuple!(A AI, B BI, C CI, D DI, E EI, F FI, G GI, H HI, I II, J JI, K KI, L LI);

pub struct UnaryTupleWrapper<A: Upstream>(A);

impl<A: Upstream, AI: IntoUpstream<A>> IntoUpstream<UnaryTupleWrapper<A>> for (AI,) {
    fn into_upstream(self) -> UnaryTupleWrapper<A> {
        UnaryTupleWrapper(self.0.into_upstream())
    }
}

impl<A: Upstream> Upstream for UnaryTupleWrapper<A> {
    type Output = (A::Output,);

    fn request(&self, ctx: Ctx, callback: Callback<(A::Output,)>) {
        self.0.request(ctx, callback.map(|o| o.map(|o| (o,))))
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

#[cfg(test)]
mod tests {
    use crate as peregrine;
    use crate::{op, run};

    #[test]
    fn tuples() {
        assert_eq!(run((1, 2)), (1, 2));
        assert_eq!(run((1, 2, op!(i!(3)))), (1, 2, 3));
    }
}
