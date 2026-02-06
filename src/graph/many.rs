use crate::graph::auto::UncachedMap;
use crate::node::Node;
use crate::{Upstream, graph::op::Op};
use array_init::array_init;
use std::sync::Arc;

pub trait Split<U> {
    type Result;

    fn split(self) -> Self::Result;
}

pub trait Merge<U> {
    fn merge(self) -> U;
}

macro_rules! impl_merge_and_split {
    ($($t:ident $f:tt),*) => {
        impl<$($t: Upstream),*> Merge<Node<Op<($($t,)*), ($($t::Output,)*), fn(($($t::Output,)*)) -> ($($t::Output,)*)>>> for ($($t,)*) {
            #[allow(non_snake_case)]
            fn merge(self) -> Node<Op<($($t,)*), ($($t::Output,)*), fn(($($t::Output,)*)) -> ($($t::Output,)*)>> {
                let ($($t,)*) = self;

                Op::new(($($t,)*), identity)
            }
        }

        impl<$($t,)* U: Upstream<Output = ($($t,)*)>> Split<(U, $($t,)*)> for U {
            type Result = ($(UncachedMap<Arc<U>, $t>,)*);

            fn split(self) -> Self::Result {
                let upstream = Arc::new(self);
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
            upstream: self,
            func: |(a,)| a,
        },)
    }
}

fn identity<T>(value: T) -> T {
    value
}

impl_merge_and_split!(A 0, B 1);
impl_merge_and_split!(A 0, B 1, C 2);
impl_merge_and_split!(A 0, B 1, C 2, D 3);
impl_merge_and_split!(A 0, B 1, C 2, D 3, E 4);
impl_merge_and_split!(A 0, B 1, C 2, D 3, E 4, F 5);
impl_merge_and_split!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
impl_merge_and_split!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);
impl_merge_and_split!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8);
impl_merge_and_split!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9);
impl_merge_and_split!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10);
impl_merge_and_split!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11);

impl<A: Upstream> Merge<(A,)> for (A,) {
    fn merge(self) -> (A,) {
        self
    }
}

impl<U: Upstream> Merge<Node<Op<Vec<U>, Vec<U::Output>, fn(Vec<U::Output>) -> Vec<U::Output>>>>
    for Vec<U>
where
    U::Output: Send + Clone + 'static,
{
    fn merge(self) -> Node<Op<Vec<U>, Vec<U::Output>, fn(Vec<U::Output>) -> Vec<U::Output>>> {
        let mut converted = Vec::with_capacity(self.len());
        for upstream in self {
            converted.push(upstream);
        }

        Op::new(converted, identity)
    }
}

impl<const N: usize, U: Upstream>
    Merge<Node<Op<[U; N], [U::Output; N], fn([U::Output; N]) -> [U::Output; N]>>> for [U; N]
where
    U::Output: Send + Clone + 'static,
{
    fn merge(self) -> Node<Op<[U; N], [U::Output; N], fn([U::Output; N]) -> [U::Output; N]>> {
        let mut iter = self.into_iter();
        let converted = array_init(move |_| iter.next().unwrap());

        Op::new(converted, identity)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use crate as peregrine;
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
