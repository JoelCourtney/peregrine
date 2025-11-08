use array_init::array_init;
use crossbeam::atomic::AtomicCell;
use std::{
    mem::transmute,
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{Callback, Ctx, Downstream, Upstream};

use super::{Cached, Invalidator};

pub trait UpstreamCollector: Send + Sync {
    type Cells: Send + Sync + 'static;
    type Result: Send;

    fn new_cells(&self) -> Self::Cells;
    fn request<'s>(
        &self,
        ctx: Ctx<'_, 's>,
        cells: &Self::Cells,
        counter: &AtomicU32,
        downstream: &'s dyn Downstream,
    ) where
        Self: 's;
    fn get(
        &self,
        cells: &Self::Cells,
        invalidator_factory: impl Fn() -> Invalidator,
    ) -> (Self::Result, CollectionStatus);
}

pub enum CollectionStatus {
    Constant,
    Variable,
    Revalidated,
}

impl UpstreamCollector for () {
    type Cells = ();
    type Result = ();

    fn new_cells(&self) -> Self::Cells {}

    fn request<'s>(
        &self,
        ctx: Ctx<'_, 's>,
        _cells: &(),
        _counter: &AtomicU32,
        downstream: &'s dyn Downstream,
    ) where
        Self: 's,
    {
        downstream.run(ctx);
    }

    fn get(
        &self,
        _cells: &Self::Cells,
        _factory: impl Fn() -> Invalidator,
    ) -> ((), CollectionStatus) {
        ((), CollectionStatus::Constant)
    }
}

macro_rules! impl_upstream_collector_tuple {
    ($($t:ident $u:ident $c:ident),*) => {
        impl<$($t: Upstream),*> UpstreamCollector for ($($t,)*)
        where $($t::Output: Clone),* {
            type Cells = ($(AtomicCell<Option<Cached<$t::Output>>>,)*);
            type Result = ($($t::Output,)*);

            fn new_cells(&self) -> Self::Cells {
                Default::default()
            }

            #[allow(unused)]
            fn request<'s>(&self, ctx: Ctx<'_, 's>, cells: &Self::Cells, counter: &AtomicU32, downstream: &'s dyn Downstream) where Self: 's {
                let mut count = $( one::<$t>() +)* 0;
                counter.store(count, Ordering::Relaxed);

                let ($($u,)*) = &self;
                let ($($c,)*) = &cells;

                let run_count = ctx.run_count;

                $(
                    count -= 1;
                    let callback = unsafe {
                         Callback::new(downstream, transmute::<&AtomicCell<Option<Cached<_>>>, &'s AtomicCell<Option<Cached<_>>>>($c))
                    };
                    if count == 0 {
                        $u.request(ctx, callback);
                    } else {
                        let upstream = unsafe {
                            transmute::<&$t, &'s $t>(&$u)
                        };
                        ctx.scope.spawn(move |scope| upstream.request(Ctx { scope, run_count }, callback))
                    }
                )*
            }

            fn get(&self, cells: &Self::Cells, factory: impl Fn() -> Invalidator) -> (Self::Result, CollectionStatus) {
                let ($($c,)*) = &cells;
                let mut constant = true;
                let mut revalidate = true;
                let tuple = ($(
                    {
                        let mut cached = $c.take().unwrap();
                        let result = match &mut cached {
                            Cached::Constant(v) => v.clone(),
                            Cached::Variable { value, senders, revalidated } => {
                                for sender in senders.drain(..) {
                                    sender.send(factory()).unwrap();
                                }
                                constant = false;
                                revalidate = revalidate && *revalidated;
                                value.clone()
                            }
                        };
                        $c.store(Some(cached));
                        result
                    },
                )*);

                (
                    tuple,
                    match (constant, revalidate) {
                        (true, _) => CollectionStatus::Constant,
                        (_, true) => CollectionStatus::Revalidated,
                        _ => CollectionStatus::Variable,
                    },
                )
            }
        }
    };
}

/// I am a mad god of logic and these
/// feeble macros cannot stop me.
#[inline(always)]
#[allow(clippy::extra_unused_type_parameters)]
fn one<T>() -> u32 {
    1
}

impl_upstream_collector_tuple!(A a_up a_cell);
impl_upstream_collector_tuple!(A a_up a_cell, B b_up b_cell);
impl_upstream_collector_tuple!(A a_up a_cell, B b_up b_cell, C c_up c_cell);
impl_upstream_collector_tuple!(A a_up a_cell, B b_up b_cell, C c_up c_cell, D d_up d_cell);
impl_upstream_collector_tuple!(A a_up a_cell, B b_up b_cell, C c_up c_cell, D d_up d_cell, E e_up e_cell);
impl_upstream_collector_tuple!(A a_up a_cell, B b_up b_cell, C c_up c_cell, D d_up d_cell, E e_up e_cell, F f_up f_cell);
impl_upstream_collector_tuple!(A a_up a_cell, B b_up b_cell, C c_up c_cell, D d_up d_cell, E e_up e_cell, F f_up f_cell, G g_up g_cell);
impl_upstream_collector_tuple!(A a_up a_cell, B b_up b_cell, C c_up c_cell, D d_up d_cell, E e_up e_cell, F f_up f_cell, G g_up g_cell, H h_up h_cell);
impl_upstream_collector_tuple!(A a_up a_cell, B b_up b_cell, C c_up c_cell, D d_up d_cell, E e_up e_cell, F f_up f_cell, G g_up g_cell, H h_up h_cell, I i_up i_cell);
impl_upstream_collector_tuple!(A a_up a_cell, B b_up b_cell, C c_up c_cell, D d_up d_cell, E e_up e_cell, F f_up f_cell, G g_up g_cell, H h_up h_cell, I i_up i_cell, J j_up j_cell);
impl_upstream_collector_tuple!(A a_up a_cell, B b_up b_cell, C c_up c_cell, D d_up d_cell, E e_up e_cell, F f_up f_cell, G g_up g_cell, H h_up h_cell, I i_up i_cell, J j_up j_cell, K k_up k_cell);
impl_upstream_collector_tuple!(A a_up a_cell, B b_up b_cell, C c_up c_cell, D d_up d_cell, E e_up e_cell, F f_up f_cell, G g_up g_cell, H h_up h_cell, I i_up i_cell, J j_up j_cell, K k_up k_cell, L l_up l_cell);

impl<U: Upstream<Output = O>, O: Send + Clone + 'static> UpstreamCollector for Vec<U> {
    type Cells = Vec<AtomicCell<Option<Cached<O>>>>;
    type Result = Vec<O>;

    fn new_cells(&self) -> Self::Cells {
        Vec::with_capacity(self.len())
    }

    fn request<'s>(
        &self,
        ctx: Ctx<'_, 's>,
        cells: &Self::Cells,
        counter: &AtomicU32,
        downstream: &'s dyn Downstream,
    ) where
        Self: 's,
    {
        if self.is_empty() {
            if downstream.should_run() {
                downstream.run(ctx);
            }
            return;
        }

        counter.store(self.len() as u32, Ordering::Relaxed);

        let mut iter = self.iter().enumerate();
        let (_, first) = iter.next().unwrap();

        let run_count = ctx.run_count;

        for (i, upstream) in iter {
            let callback = unsafe {
                Callback::new(
                    downstream,
                    transmute::<&AtomicCell<Option<Cached<_>>>, &'s AtomicCell<Option<Cached<_>>>>(
                        &cells[i],
                    ),
                )
            };
            let upstream = unsafe { transmute::<&U, &'s U>(upstream) };
            ctx.scope
                .spawn(move |scope| upstream.request(Ctx { scope, run_count }, callback))
        }

        first.request(ctx, unsafe {
            Callback::new(
                downstream,
                transmute::<&AtomicCell<Option<Cached<_>>>, &'s AtomicCell<Option<Cached<_>>>>(
                    &cells[0],
                ),
            )
        });
    }

    fn get(
        &self,
        cells: &Self::Cells,
        factory: impl Fn() -> Invalidator,
    ) -> (Vec<O>, CollectionStatus) {
        let mut constant = true;
        let mut revalidate = true;
        let result = cells
            .iter()
            .map(|c| {
                let mut cached = c.take().unwrap();
                let result = match &mut cached {
                    Cached::Constant(v) => v.clone(),
                    Cached::Variable {
                        value,
                        senders,
                        revalidated,
                    } => {
                        for sender in senders.drain(..) {
                            sender.send(factory()).unwrap();
                        }
                        constant = false;
                        revalidate = revalidate && *revalidated;
                        value.clone()
                    }
                };
                c.store(Some(cached));
                result
            })
            .collect();

        (
            result,
            match (constant, revalidate) {
                (true, _) => CollectionStatus::Constant,
                (_, true) => CollectionStatus::Revalidated,
                _ => CollectionStatus::Variable,
            },
        )
    }
}

impl<const N: usize, U: Upstream<Output = O>, O: Send + Clone + 'static> UpstreamCollector
    for [U; N]
{
    type Cells = [AtomicCell<Option<Cached<O>>>; N];
    type Result = [O; N];

    fn new_cells(&self) -> Self::Cells {
        array_init(|_| AtomicCell::new(None))
    }

    fn request<'s>(
        &self,
        ctx: Ctx<'_, 's>,
        cells: &Self::Cells,
        counter: &AtomicU32,
        downstream: &'s dyn Downstream,
    ) where
        Self: 's,
    {
        if self.is_empty() {
            if downstream.should_run() {
                downstream.run(ctx);
            }
            return;
        }

        counter.store(self.len() as u32, Ordering::Relaxed);

        let mut iter = self.iter().enumerate();
        let (_, first) = iter.next().unwrap();

        let run_count = ctx.run_count;

        for (i, upstream) in iter {
            let callback = unsafe {
                Callback::new(
                    downstream,
                    transmute::<&AtomicCell<Option<Cached<_>>>, &'s AtomicCell<Option<Cached<_>>>>(
                        &cells[i],
                    ),
                )
            };
            let upstream = unsafe { transmute::<&U, &'s U>(upstream) };
            ctx.scope
                .spawn(move |scope| upstream.request(Ctx { scope, run_count }, callback))
        }

        first.request(ctx, unsafe {
            Callback::new(
                downstream,
                transmute::<&AtomicCell<Option<Cached<_>>>, &'s AtomicCell<Option<Cached<_>>>>(
                    &cells[0],
                ),
            )
        });
    }

    fn get(
        &self,
        cells: &Self::Cells,
        factory: impl Fn() -> Invalidator,
    ) -> ([O; N], CollectionStatus) {
        let mut constant = true;
        let mut revalidate = true;
        let result = array_init(|i| {
            let mut cached = cells[i].take().unwrap();
            let result = match &mut cached {
                Cached::Constant(v) => v.clone(),
                Cached::Variable {
                    value,
                    senders,
                    revalidated,
                } => {
                    for sender in senders.drain(..) {
                        sender.send(factory()).unwrap();
                    }
                    constant = false;
                    revalidate = revalidate && *revalidated;
                    value.clone()
                }
            };
            cells[i].store(Some(cached));
            result
        });

        (
            result,
            match (constant, revalidate) {
                (true, _) => CollectionStatus::Constant,
                (_, true) => CollectionStatus::Revalidated,
                _ => CollectionStatus::Variable,
            },
        )
    }
}
