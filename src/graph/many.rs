use crate::graph::auto::UncachedMap;
use crate::{Callback, Ctx, IntoUpstream, Upstream, graph::op::Op};
use array_init::array_init;
use std::sync::Arc;

use super::NodeId;

pub trait Split<U> {
    type Result;

    fn split(self) -> Self::Result;
}

pub trait Merge<U> {
    fn merge(self) -> U;
}

macro_rules! impl_into_upstream_for_tuple {
    ($($t:ident $t_i:ident $f:tt),*) => {
        impl<$($t: Upstream, $t_i: IntoUpstream<$t>),*> Merge<Op<($($t,)*), ($($t::Output,)*), fn(($($t::Output,)*)) -> ($($t::Output,)*)>> for ($($t_i,)*) {
            #[allow(non_snake_case)]
            fn merge(self) -> Op<($($t,)*), ($($t::Output,)*), fn(($($t::Output,)*)) -> ($($t::Output,)*)> {
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

impl<A: Upstream, AI: IntoUpstream<A>> Merge<UnaryTupleWrapper<A>> for (AI,) {
    fn merge(self) -> UnaryTupleWrapper<A> {
        UnaryTupleWrapper(self.0.into_upstream())
    }
}

impl<A: Upstream> Upstream for UnaryTupleWrapper<A> {
    type Output = (A::Output,);

    fn node_id(&self) -> Option<NodeId> {
        self.0.node_id()
    }
    fn request<'s>(&self, ctx: Ctx<'_, 's>, callback: Callback<'s, (A::Output,)>)
    where
        Self: 's,
    {
        self.0.request(ctx, callback.map(|o| o.map(|o| (o,))))
    }
}

impl<U: Upstream, IU: IntoUpstream<U>>
    Merge<Op<Vec<U>, Vec<U::Output>, fn(Vec<U::Output>) -> Vec<U::Output>>> for Vec<IU>
where
    U::Output: Send + Clone + 'static,
{
    fn merge(self) -> Op<Vec<U>, Vec<U::Output>, fn(Vec<U::Output>) -> Vec<U::Output>> {
        let mut converted = Vec::with_capacity(self.len());
        let mut node_ids = Vec::with_capacity(self.len());
        for upstream in self {
            let upstream = upstream.into_upstream();
            if let Some(id) = upstream.node_id() {
                node_ids.push(id);
            }
            converted.push(upstream);
        }

        Op::new(converted, identity, node_ids)
    }
}

impl<const N: usize, U: Upstream, IU: IntoUpstream<U>>
    Merge<Op<[U; N], [U::Output; N], fn([U::Output; N]) -> [U::Output; N]>> for [IU; N]
where
    U::Output: Send + Clone + 'static,
{
    fn merge(self) -> Op<[U; N], [U::Output; N], fn([U::Output; N]) -> [U::Output; N]> {
        let mut iter = self.into_iter();
        let converted = array_init(move |_| iter.next().unwrap().into_upstream());
        let node_ids: [_; N] = array_init(|i| converted[i].node_id());

        Op::new(converted, identity, node_ids.into_iter().flatten())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use crate as desparrow;
    use crate::graph::many::Merge;
    use crate::{op, run};

    use super::Split;

    #[test]
    fn merge() {
        assert_eq!(run((1, 2).merge()), (1, 2));
        assert_eq!(run((1, 2, op!(i!(3))).merge()), (1, 2, 3));
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

    #[test]
    fn vector() {
        assert_eq!(run(vec![1, 2]), vec![1, 2]);
    }
}
