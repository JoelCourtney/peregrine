use std::{
    mem::transmute,
    sync::atomic::{AtomicU32, Ordering},
};

use crossbeam::atomic::AtomicCell;

use crate::{Callback, Ctx, Downstream, Upstream, cache::Cached};

pub(crate) struct Sink<O> {
    output: AtomicCell<Option<Cached<O>>>,
    _counter: AtomicU32,
}

impl<O: Send> Sink<O> {
    pub(crate) fn new() -> Self {
        Self {
            output: AtomicCell::new(None),
            _counter: AtomicU32::new(0),
        }
    }

    pub(crate) fn as_callback(&self) -> Callback<O> {
        unsafe {
            Callback::new(
                transmute::<&dyn Downstream, &'static dyn Downstream>(self),
                transmute::<&AtomicCell<Option<Cached<O>>>, &'static AtomicCell<Option<Cached<O>>>>(
                    &self.output,
                ),
            )
        }
    }

    pub(crate) fn open(self) -> O {
        self.output
            .take()
            .expect("Cannot open sink, no value was provided")
            .open()
    }
}

impl<O: Send> Downstream for Sink<O> {
    fn should_run(&self) -> bool {
        true
    }

    fn run(&self, _: Ctx) {}
}

pub(crate) struct Callbacks<O: 'static> {
    vec: Vec<Callback<O>>,
}

impl<O: 'static> Default for Callbacks<O> {
    fn default() -> Self {
        Self {
            vec: Default::default(),
        }
    }
}

impl<O: Clone> Callbacks<O> {
    pub fn add(&mut self, callback: Callback<O>) {
        self.vec.push(callback);
    }

    pub fn run(self, ctx: Ctx, mut value_factory: impl FnMut() -> Cached<O>) {
        if self.vec.is_empty() {
            return;
        }

        let mut to_run = self.vec.into_iter().filter_map(|c| {
            c.output.store(value_factory());
            if c.downstream.should_run() {
                Some(c.downstream)
            } else {
                None
            }
        });

        let first = to_run.next();

        for downstream in to_run {
            ctx.scope.spawn(move |scope| downstream.run(Ctx { scope }))
        }
        if let Some(downstream) = first {
            downstream.run(ctx);
        }
    }
}

pub struct UpstreamCollector<U, C> {
    upstreams: U,
    outputs: C,
}

pub trait UpstreamCollectorExt<U> {
    type Combined;

    fn new(upstreams: U) -> Self;
    fn request(&self, ctx: Ctx, counter: &AtomicU32, downstream: &'static dyn Downstream);
    fn get(&self) -> Cached<Self::Combined>;
}

macro_rules! impl_upstream_collector_tuple {
    ($($t:ident $u:ident $c:ident),*) => {
        impl<$($t: Upstream + 'static),*> UpstreamCollectorExt<($($t,)*),> for UpstreamCollector<($($t,)*), ($(AtomicCell<Option<Cached<$t::Output>>>,)*)>
        where $($t::Output: Clone),* {
            type Combined = ($($t::Output,)*);

            fn new(upstreams: ($($t,)*)) -> Self {
                Self {
                    upstreams,
                    outputs: Default::default()
                }
            }

            #[allow(unused)]
            fn request(&self, ctx: Ctx, counter: &AtomicU32, downstream: &'static dyn Downstream) {
                let mut count = $( one::<$t>() +)* 0;
                counter.store(count, Ordering::Relaxed);

                let ($($u,)*) = &self.upstreams;
                let ($($c,)*) = &self.outputs;

                $(
                    count -= 1;
                    let callback = unsafe {
                         Callback::new(downstream, transmute::<&AtomicCell<Option<Cached<_>>>, &'static AtomicCell<Option<Cached<_>>>>($c))
                    };
                    if count == 0 {
                        $u.request(ctx, callback);
                    } else {
                        let upstream = unsafe {
                            transmute::<&$t, &'static $t>(&$u)
                        };
                        ctx.scope.spawn(move |scope| upstream.request(Ctx { scope }, callback))
                    }
                )*
            }

            fn get(&self) -> Cached<($($t::Output,)*)> {
                let mut combined_senders = vec![];
                let ($($c,)*) = &self.outputs;
                let mut constant = true;
                let mut revalidate = true;
                let tuple = ($(
                    {
                        let mut cached = $c.take().unwrap();
                        let result = match &mut cached {
                            Cached::Constant(v) => v.clone(),
                            Cached::Variable { value, senders, revalidated } => {
                                constant = false;
                                revalidate = revalidate && *revalidated;
                                combined_senders.extend(senders.drain(..));
                                value.clone()
                            }
                        };
                        $c.store(Some(cached));
                        result
                    },
                )*);

                if constant {
                    Cached::Constant(tuple)
                } else {
                    Cached::Variable {
                        value: tuple,
                        senders: combined_senders,
                        revalidated: revalidate
                    }
                }
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
