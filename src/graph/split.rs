use crate::Upstream;
use crate::graph::auto::UncachedMap;
use std::sync::Arc;

pub trait Split<U> {
    type Result;

    fn split(self) -> Self::Result;
}

macro_rules! impl_split {
    ($($t:ident $f:tt),*) => {
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

impl_split!(A 0, B 1);
impl_split!(A 0, B 1, C 2);
impl_split!(A 0, B 1, C 2, D 3);
impl_split!(A 0, B 1, C 2, D 3, E 4);
impl_split!(A 0, B 1, C 2, D 3, E 4, F 5);
impl_split!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
impl_split!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);
impl_split!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8);
impl_split!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9);
impl_split!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10);
impl_split!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11);

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use crate as peregrine;
    use crate::data::Source;
    use crate::graph::op::Op;
    use crate::{op, run};

    use super::Split;

    #[test]
    fn merge() {
        assert_eq!(run(Op::collect((1, 2))), (1, 2));
        assert_eq!(run(Op::collect((1, 2, op!(i!(3))))), (1, 2, 3));
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
        assert_eq!(run(Op::collect(vec![Source(1), Source(2)])), vec![1, 2]);
    }
}
