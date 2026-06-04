use std::ops::{Add, BitAnd, BitOr, BitXor, Div, Mul, Neg, Not, Rem, Shl, Shr, Sub};

use derive_more::{Deref, DerefMut};

use crate::graph::auto::UncachedMap;
use crate::op;
use crate::{Data, Upstream, graph::op::Op};

#[derive(Copy, Clone, Debug, Deref, DerefMut, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Node<T>(pub T);

impl<T> Upstream for Node<T>
where
    T: Upstream,
{
    type Output = T::Output;

    fn request<'s>(&'s self, ctx: crate::Ctx<'_, '_, 's>, callback: crate::Callback<'s, Self::Output>)
    where
        Self: 's,
    {
        self.0.request(ctx, callback);
    }
}

macro_rules! impl_binary_op {
    ($trait:ident $fun:ident $sym:tt) => {
        impl<T, U> $trait<U> for Node<T>
        where
            T: Upstream,
            U: Upstream,
            T::Output: $trait<U::Output>,
            <T::Output as $trait<U::Output>>::Output: Data,
        {
            type Output = Node<
                Op<
                    (Node<T>, U),
                    <T::Output as $trait<U::Output>>::Output,
                    fn((T::Output, U::Output)) -> <T::Output as $trait<U::Output>>::Output,
                >,
            >;

            fn $fun(self, other: U) -> Self::Output {
                use crate as peregrine;
                op! {
                    i!(self) $sym i!(other)
                }
            }
        }
    };
}

impl_binary_op!(Add add +);
impl_binary_op!(Sub sub -);
impl_binary_op!(Mul mul *);
impl_binary_op!(Div div /);
impl_binary_op!(BitAnd bitand &);
impl_binary_op!(BitOr bitor |);
impl_binary_op!(BitXor bitxor ^);
impl_binary_op!(Rem rem %);
impl_binary_op!(Shl shl <<);
impl_binary_op!(Shr shr >>);

impl<T> Neg for Node<T>
where
    T: Upstream,
    T::Output: Neg,
    <T::Output as Neg>::Output: Data,
{
    type Output = Node<UncachedMap<T, <T::Output as Neg>::Output>>;

    fn neg(self) -> Self::Output {
        Node(UncachedMap {
            upstream: self.0,
            func: <T::Output as Neg>::neg,
        })
    }
}

impl<T> Not for Node<T>
where
    T: Upstream,
    T::Output: Not,
    <T::Output as Not>::Output: Data,
{
    type Output = Node<UncachedMap<T, <T::Output as Not>::Output>>;

    fn not(self) -> Self::Output {
        Node(UncachedMap {
            upstream: self.0,
            func: <T::Output as Not>::not,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate as peregrine;
    use crate::graph::variable::Var;
    use crate::run;

    use super::*;

    #[test]
    fn node_binary_ops() {
        let node1 = op! { 1 + 2 };
        let node2 = Var::new(5);
        let result = node1 + node2 - 4;
        assert_eq!(run(result), 4);
    }

    #[test]
    fn node_unary_ops() {
        let node = op! { 1 + 2 };
        assert_eq!(run(-node), -3);
    }
}
