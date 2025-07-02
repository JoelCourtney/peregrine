use crate::internal::operation::Node;
use crate::internal::placement::{DenseTime, Placement};
use crate::internal::timeline::epoch_to_duration;
use bumpalo_herd::Member;
use hifitime::{Duration, Epoch as Time};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::ops::AddAssign;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub trait OpsReceiver<'v, 'o: 'v> {
    /// Add an operation at the current time.
    fn push<N: Node<'o> + 'o>(&mut self, op_ctor: impl FnOnce(Placement<'o>) -> N);

    /// Move the ops cursor later in time by a relative delay.
    ///
    /// Can be either a [Duration] or a dynamic `delay!` (not yet implemented).
    fn wait<D>(&mut self, delay: D)
    where
        Placement<'o>: AddAssign<(D, &'v Member<'o>)>;

    /// Fast-forwards to the given time if it is in the future.
    ///
    /// Does nothing if it is in the past.
    fn wait_until(&mut self, _time: Time);

    /// Sets the cursor to the given time.
    fn goto(&mut self, time: Time);
}

/// A cursor and operations aggregator for inserting ops into the plan.
///
/// Within the context of an [Activity], you can move the cursor around
/// in time (including backward), then add operations with [OpsReceiver::push]
/// or the add-assign (`+=`) operator.
///
/// ## Delegating & Mutability
///
/// Since this struct contains some internal state (the location of the cursor
/// in the plan), you might need to be cautious about delegating to helper functions
/// that move the cursor around. If you want those cursor changes to be reflected
/// back in the original scope, the helper function should accept `&mut Ops` as argument;
/// if not, the helper function should take just `Ops`. This struct is [Copy], and making
/// a copy will make a new cursor that can be moved around independently.
///
/// Alternatively, if you want to decide the mutability at the call site instead of the
/// function declaration, the function can accept `impl OpsReceiver<'o>`. Both `Ops` and
/// `&mut Ops` implement [OpsReceiver], and so the function caller can decide which to pass.
#[derive(Clone)]
pub struct Ops<'v, 'o: 'v> {
    /// The current placement time that operations will be inserted at.
    pub(crate) placement: Placement<'o>,
    /// An arena allocator to store operations in.
    pub(crate) bump: &'v Member<'o>,
    /// The aggregator for operation references. The underlying [Vec]
    /// is unwrapped by the [Plan][crate::Plan] after the activity is done.
    pub(crate) operations: &'v RefCell<Vec<&'o dyn Node<'o>>>,
    pub(crate) order: Arc<AtomicU64>,
}

impl<'v, 'o: 'v> Ops<'v, 'o> {
    #[doc(hidden)]
    pub fn new(
        placement: Placement<'o>,
        bump: &'v Member<'o>,
        operations: &'v RefCell<Vec<&'o dyn Node<'o>>>,
        order: Arc<AtomicU64>,
    ) -> Self {
        Self {
            placement,
            bump,
            operations,
            order,
        }
    }
}

impl<'v, 'o: 'v> OpsReceiver<'v, 'o> for Ops<'v, 'o> {
    #[inline]
    fn push<N: Node<'o> + 'o>(&mut self, op_ctor: impl FnOnce(Placement<'o>) -> N) {
        self.placement
            .set_order(self.order.fetch_add(1, Ordering::SeqCst));
        let op = self.bump.alloc(op_ctor(self.placement));
        self.operations.borrow_mut().push(op);
    }

    fn wait<D>(&mut self, delay: D)
    where
        Placement<'o>: AddAssign<(D, &'v Member<'o>)>,
    {
        self.placement += (delay, self.bump);
    }

    fn wait_until(&mut self, _time: Time) {
        todo!()
    }

    fn goto(&mut self, time: Time) {
        self.placement = Placement::Static(DenseTime {
            when: epoch_to_duration(time),
            order: 0,
        });
    }
}

impl<'v, 'o: 'v> OpsReceiver<'v, 'o> for &mut Ops<'v, 'o> {
    fn push<N: Node<'o> + 'o>(&mut self, op_ctor: impl FnOnce(Placement<'o>) -> N) {
        (*self).push(op_ctor);
    }

    fn wait<D>(&mut self, delay: D)
    where
        Placement<'o>: AddAssign<(D, &'v Member<'o>)>,
    {
        (*self).wait(delay);
    }

    fn wait_until(&mut self, time: Time) {
        (*self).wait_until(time);
    }

    fn goto(&mut self, time: Time) {
        (*self).goto(time);
    }
}

impl<'o, N: Node<'o> + 'o, F: FnOnce(Placement<'o>) -> N> AddAssign<F> for Ops<'_, 'o> {
    fn add_assign(&mut self, rhs: F) {
        self.push(rhs);
    }
}

impl<'o, N: Node<'o> + 'o, F: FnOnce(Placement<'o>) -> N> AddAssign<F> for &mut Ops<'_, 'o> {
    fn add_assign(&mut self, rhs: F) {
        self.push(rhs);
    }
}

/// An activity, which produces into a statically-known set of operations.
/// Returns the activity's final duration and may produce errors.
#[cfg_attr(feature = "serde", typetag::serde(tag = "type"))]
pub trait Activity: Send + Sync {
    fn run<'o>(&'o self, ops: Ops<'_, 'o>) -> anyhow::Result<Duration>;
}

/// A unique activity ID.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, Debug)]
pub struct ActivityId(u32);

impl ActivityId {
    pub fn new(id: u32) -> ActivityId {
        ActivityId(id)
    }
}
