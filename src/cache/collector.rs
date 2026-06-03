use array_init::array_init;
use std::{
    cell::UnsafeCell,
    mem::{take, transmute},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use crate::{Callback, Ctx, Downstream, Upstream};

use super::Cached;

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
    fn get<F: FnOnce() + Send + 'static>(
        cells: &Arc<Self::Cells>,
        invalidator_factory: impl Fn() -> F,
    ) -> (Self::Result, CollectionStatus);
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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

    fn get<F: FnOnce() + Send>(
        _cells: &Arc<Self::Cells>,
        _factory: impl Fn() -> F,
    ) -> ((), CollectionStatus) {
        ((), CollectionStatus::Constant)
    }
}

pub struct OutputCell<T>(UnsafeCell<Option<Cached<T>>>);

impl<T> Default for OutputCell<T> {
    fn default() -> Self {
        OutputCell(UnsafeCell::new(None))
    }
}

impl<T> OutputCell<T> {
    fn is_some(&self) -> bool {
        unsafe { (*self.0.get()).is_some() }
    }

    fn get_drain(&self) -> Option<Cached<T>>
    where
        T: Clone,
    {
        let value = unsafe { (*self.0.get()).as_mut() };
        match value {
            Some(Cached::Constant(v)) => Some(Cached::Constant(v.clone())),
            Some(Cached::Variable {
                value,
                senders,
                revalidated,
            }) => Some(Cached::Variable {
                value: value.clone(),
                senders: take(senders),
                revalidated: *revalidated,
            }),
            None => None,
        }
    }

    fn clear(&self) {
        unsafe {
            (*self.0.get()) = None;
        }
    }

    pub(crate) fn take(&self) -> Option<Cached<T>> {
        unsafe { (*self.0.get()).take() }
    }

    pub(crate) fn store(&self, value: Cached<T>) {
        unsafe {
            (*self.0.get()) = Some(value);
        }
    }
}

unsafe impl<T: Send> Sync for OutputCell<T> {}
unsafe impl<T: Send> Send for OutputCell<T> {}

macro_rules! impl_upstream_collector_tuple {
    ($($t:ident $u:ident $c:ident $index:tt),*) => {
        impl<$($t: Upstream),*> UpstreamCollector for ($($t,)*)
        where $($t::Output: Clone),* {
            type Cells = ($(OutputCell<$t::Output>,)*);
            type Result = ($($t::Output,)*);

            fn new_cells(&self) -> Self::Cells {
                Default::default()
            }

            fn request<'s>(&self, ctx: Ctx<'_, 's>, cells: &Self::Cells, counter: &AtomicU32, downstream: &'s dyn Downstream) where Self: 's {
                let ($($u,)*) = &self;
                let ($($c,)*) = &cells;

                let mut count = $( if $c.is_some() { 0 } else { 1 } +)* 0;
                counter.store(count, Ordering::Relaxed);

                $(
                    if !$c.is_some() {
                        count -= 1;
                        let callback = unsafe {
                             Callback::new(downstream, transmute::<&OutputCell<_>, &'s OutputCell<_>>($c))
                        };
                        let upstream = unsafe {
                            transmute::<&$t, &'s $t>($u)
                        };
                        if count == 0 {
                            ctx.run(move |ctx| upstream.request(ctx, callback));
                        } else {
                            ctx.spawn(move |ctx| upstream.request(ctx, callback));
                        }
                    }
                )*
            }

            fn get<FUNC: FnOnce() + Send + 'static>(cells: &Arc<Self::Cells>, factory: impl Fn() -> FUNC) -> (Self::Result, CollectionStatus) {
                let ($($c,)*) = &**cells;
                let mut constant = true;
                let mut revalidate = true;
                let tuple = ($(
                    {
                        let cached = $c.get_drain().expect("Output cell should be populated");
                        match cached {
                            Cached::Constant(v) => v.clone(),
                            Cached::Variable { value, senders, revalidated } => {
                                for sender in senders {
                                    let cells_weak = Arc::downgrade(cells);
                                    let base_invalidator = factory();
                                    let invalidator = move || {
                                        base_invalidator();
                                        if let Some(cells) = cells_weak.upgrade() {
                                            cells.$index.clear();
                                        }
                                    };
                                    sender.send(Box::new(invalidator)).expect("Upstream cache was dropped before an invalidator could be sent");
                                }
                                constant = false;
                                revalidate = revalidate && revalidated;
                                value.clone()
                            }
                        }
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

impl_upstream_collector_tuple!(A a_up a_cell 0);
impl_upstream_collector_tuple!(A a_up a_cell 0, B b_up b_cell 1);
impl_upstream_collector_tuple!(A a_up a_cell 0, B b_up b_cell 1, C c_up c_cell 2);
impl_upstream_collector_tuple!(A a_up a_cell 0, B b_up b_cell 1, C c_up c_cell 2, D d_up d_cell 3);
impl_upstream_collector_tuple!(A a_up a_cell 0, B b_up b_cell 1, C c_up c_cell 2, D d_up d_cell 3, E e_up e_cell 4);
impl_upstream_collector_tuple!(A a_up a_cell 0, B b_up b_cell 1, C c_up c_cell 2, D d_up d_cell 3, E e_up e_cell 4, F f_up f_cell 5);
impl_upstream_collector_tuple!(A a_up a_cell 0, B b_up b_cell 1, C c_up c_cell 2, D d_up d_cell 3, E e_up e_cell 4, F f_up f_cell 5, G g_up g_cell 6);
impl_upstream_collector_tuple!(A a_up a_cell 0, B b_up b_cell 1, C c_up c_cell 2, D d_up d_cell 3, E e_up e_cell 4, F f_up f_cell 5, G g_up g_cell 6, H h_up h_cell 7);
impl_upstream_collector_tuple!(A a_up a_cell 0, B b_up b_cell 1, C c_up c_cell 2, D d_up d_cell 3, E e_up e_cell 4, F f_up f_cell 5, G g_up g_cell 6, H h_up h_cell 7, I i_up i_cell 8);
impl_upstream_collector_tuple!(A a_up a_cell 0, B b_up b_cell 1, C c_up c_cell 2, D d_up d_cell 3, E e_up e_cell 4, F f_up f_cell 5, G g_up g_cell 6, H h_up h_cell 7, I i_up i_cell 8, J j_up j_cell 9);
impl_upstream_collector_tuple!(A a_up a_cell 0, B b_up b_cell 1, C c_up c_cell 2, D d_up d_cell 3, E e_up e_cell 4, F f_up f_cell 5, G g_up g_cell 6, H h_up h_cell 7, I i_up i_cell 8, J j_up j_cell 9, K k_up k_cell 10);
impl_upstream_collector_tuple!(A a_up a_cell 0, B b_up b_cell 1, C c_up c_cell 2, D d_up d_cell 3, E e_up e_cell 4, F f_up f_cell 5, G g_up g_cell 6, H h_up h_cell 7, I i_up i_cell 8, J j_up j_cell 9, K k_up k_cell 10, L l_up l_cell 11);

impl<U: Upstream<Output = O>, O: Send + Clone + 'static> UpstreamCollector for Vec<U> {
    type Cells = Vec<OutputCell<O>>;
    type Result = Vec<O>;

    fn new_cells(&self) -> Self::Cells {
        (0..self.len()).map(|_| OutputCell::default()).collect()
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

        let mut count = 0;

        debug_assert_eq!(self.len(), cells.len());

        let mut zipped = self.iter().zip(cells.iter()).filter(|(_, c)| {
            if c.is_some() {
                false
            } else {
                count += 1;
                true
            }
        });

        let Some((first_upstream, first_cell)) = zipped.next() else {
            if downstream.should_run() {
                downstream.run(ctx);
            }
            return;
        };

        for (upstream, cell) in zipped {
            let callback = unsafe {
                Callback::new(
                    downstream,
                    transmute::<&OutputCell<_>, &'s OutputCell<_>>(cell),
                )
            };
            let upstream = unsafe { transmute::<&U, &'s U>(upstream) };
            ctx.spawn(move |ctx| upstream.request(ctx, callback));
        }

        counter.store(count, Ordering::Relaxed);

        let callback = unsafe {
            Callback::new(
                downstream,
                transmute::<&OutputCell<_>, &'s OutputCell<_>>(first_cell),
            )
        };
        let first = unsafe { transmute::<&U, &'s U>(first_upstream) };
        ctx.run(move |ctx| first.request(ctx, callback));
    }

    fn get<F: FnOnce() + Send + 'static>(
        cells: &Arc<Self::Cells>,
        factory: impl Fn() -> F,
    ) -> (Vec<O>, CollectionStatus) {
        let mut constant = true;
        let mut revalidate = true;
        let result = cells
            .iter()
            .map(|c| {
                let cached = c.get_drain().expect("Output cell should be populated");
                match cached {
                    Cached::Constant(v) => v.clone(),
                    Cached::Variable {
                        value,
                        senders,
                        revalidated,
                    } => {
                        for sender in senders {
                            sender.send(Box::new(factory())).expect(
                                "Upstream cache was dropped before an invalidator could be sent",
                            );
                        }
                        constant = false;
                        revalidate = revalidate && revalidated;
                        value.clone()
                    }
                }
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
    type Cells = [OutputCell<O>; N];
    type Result = [O; N];

    fn new_cells(&self) -> Self::Cells {
        array_init(|_| OutputCell::default())
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
        let mut count = 0;

        let mut zipped = self.iter().zip(cells.iter()).filter(|(_, c)| {
            if c.is_some() {
                false
            } else {
                count += 1;
                true
            }
        });

        let Some((first_upstream, first_cell)) = zipped.next() else {
            if downstream.should_run() {
                downstream.run(ctx);
            }
            return;
        };

        for (upstream, cell) in zipped {
            let callback = unsafe {
                Callback::new(
                    downstream,
                    transmute::<&OutputCell<_>, &'s OutputCell<_>>(cell),
                )
            };
            let upstream = unsafe { transmute::<&U, &'s U>(upstream) };
            ctx.spawn(move |ctx| upstream.request(ctx, callback));
        }

        counter.store(count, Ordering::Relaxed);

        let callback = unsafe {
            Callback::new(
                downstream,
                transmute::<&OutputCell<_>, &'s OutputCell<_>>(first_cell),
            )
        };
        let first = unsafe { transmute::<&U, &'s U>(first_upstream) };
        ctx.run(move |ctx| first.request(ctx, callback));
    }

    #[expect(clippy::indexing_slicing, reason = "Indexing necessary for array_init")]
    fn get<F: FnOnce() + Send + 'static>(
        cells: &Arc<Self::Cells>,
        factory: impl Fn() -> F,
    ) -> ([O; N], CollectionStatus) {
        let mut constant = true;
        let mut revalidate = true;
        let result = array_init(|i| {
            let cached = cells[i]
                .get_drain()
                .expect("Output cell should be populated");
            match cached {
                Cached::Constant(v) => v.clone(),
                Cached::Variable {
                    value,
                    senders,
                    revalidated,
                } => {
                    for sender in senders {
                        sender.send(Box::new(factory())).expect(
                            "Upstream cache was dropped before an invalidator could be sent",
                        );
                    }
                    constant = false;
                    revalidate = revalidate && revalidated;
                    value.clone()
                }
            }
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
