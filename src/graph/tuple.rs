use crate::graph::auto::UncachedMap;
use crate::{Cached, Callback, Ctx, IntoUpstream, Upstream, graph::op::Op};
use crossbeam::atomic::AtomicCell;
use std::sync::Arc;

use super::NodeId;

pub trait Split<U> {
    type Result;

    fn split(self) -> Self::Result;
}

macro_rules! impl_into_upstream_for_tuple {
    ($($t:ident $t_i:ident $f:tt),*) => {
        impl<$($t: Upstream + 'static, $t_i: IntoUpstream<$t>),*> IntoUpstream<Op<($($t,)*), ($(AtomicCell<Option<Cached<$t::Output>>>,)*), ($($t::Output,)*), fn(($($t::Output,)*)) -> ($($t::Output,)*)>> for ($($t_i,)*)
        where $($t::Output: Send + Clone + 'static, )* {
            #[allow(non_snake_case)]
            fn into_upstream(self) -> Op<($($t,)*), ($(AtomicCell<Option<Cached<$t::Output>>>,)*), ($($t::Output,)*), fn(($($t::Output,)*)) -> ($($t::Output,)*)> {
                let ($($t_i,)*) = self;

                let ($($t_i,)*) = ($($t_i.into_upstream()),*);
                let node_ids = [$($t_i.node_id(),)*].into_iter().filter_map(|i| i);

                Op::new(($($t_i,)*), identity, node_ids)
            }
        }

        impl<$($t,)* U: Upstream<Output = ($($t,)*)>> Split<(U, $($t,)*)> for U {
            type Result = ($(UncachedMap<Arc<U>, $t>,)*);

            fn split(self) -> Self::Result {
                let upstream = Arc::new(self.into_upstream());
                ($(
                    UncachedMap {
                        upstream: upstream.clone(),
                        func: |t| t.$f
                    },
                )*)
            }
        }
    };
}

impl<A, U: Upstream<Output = (A,)>> Split<U> for U {
    type Result = (UncachedMap<U, A>,);
    fn split(self) -> Self::Result {
        (UncachedMap {
            upstream: self.into_upstream(),
            func: |(a,)| a,
        },)
    }
}

fn identity<T>(value: T) -> T {
    value
}

impl_into_upstream_for_tuple!(A AI 0, B BI 1);
impl_into_upstream_for_tuple!(A AI 0, B BI 1, C CI 2);
impl_into_upstream_for_tuple!(A AI 0, B BI 1, C CI 2, D DI 3);
impl_into_upstream_for_tuple!(A AI 0, B BI 1, C CI 2, D DI 3, E EI 4);
impl_into_upstream_for_tuple!(A AI 0, B BI 1, C CI 2, D DI 3, E EI 4, F FI 5);
impl_into_upstream_for_tuple!(A AI 0, B BI 1, C CI 2, D DI 3, E EI 4, F FI 5, G GI 6);
impl_into_upstream_for_tuple!(A AI 0, B BI 1, C CI 2, D DI 3, E EI 4, F FI 5, G GI 6, H HI 7);
impl_into_upstream_for_tuple!(A AI 0, B BI 1, C CI 2, D DI 3, E EI 4, F FI 5, G GI 6, H HI 7, I II 8);
impl_into_upstream_for_tuple!(A AI 0, B BI 1, C CI 2, D DI 3, E EI 4, F FI 5, G GI 6, H HI 7, I II 8, J JI 9);
impl_into_upstream_for_tuple!(A AI 0, B BI 1, C CI 2, D DI 3, E EI 4, F FI 5, G GI 6, H HI 7, I II 8, J JI 9, K KI 10);
impl_into_upstream_for_tuple!(A AI 0, B BI 1, C CI 2, D DI 3, E EI 4, F FI 5, G GI 6, H HI 7, I II 8, J JI 9, K KI 10, L LI 11);

pub struct UnaryTupleWrapper<A: Upstream>(A);

impl<A: Upstream, AI: IntoUpstream<A>> IntoUpstream<UnaryTupleWrapper<A>> for (AI,) {
    fn into_upstream(self) -> UnaryTupleWrapper<A> {
        UnaryTupleWrapper(self.0.into_upstream())
    }
}

impl<A: Upstream> Upstream for UnaryTupleWrapper<A> {
    type Output = (A::Output,);

    fn node_id(&self) -> Option<NodeId> {
        self.0.node_id()
    }
    fn request(&self, ctx: Ctx, callback: Callback<(A::Output,)>) {
        self.0.request(ctx, callback.map(|o| o.map(|o| (o,))))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use crate as peregrine;
    use crate::{op, run};

    use super::Split;

    #[test]
    fn tuples() {
        assert_eq!(run((1, 2)), (1, 2));
        assert_eq!(run((1, 2, op!(i!(3)))), (1, 2, 3));
    }

    #[test]
    fn split() {
        let run_counter = Arc::new(AtomicU32::new(0));
        let cloned = run_counter.clone();
        let (a, b) = op!(
            cloned.fetch_add(1, Ordering::Relaxed);
            (i!(1), 2)
        )
        .split();

        assert_eq!(run_counter.load(Ordering::Relaxed), 0);
        assert_eq!(run(a), 1);
        assert_eq!(run_counter.load(Ordering::Relaxed), 1);
        assert_eq!(run(b), 2);
        assert_eq!(run_counter.load(Ordering::Relaxed), 1);
    }
}
