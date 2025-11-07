use array_init::array_init;
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

impl<O: Send + 'static> Sink<O> {
    pub(crate) fn new() -> Self {
        Self {
            output: AtomicCell::new(None),
            _counter: AtomicU32::new(0),
        }
    }

    pub(crate) fn as_callback<'s>(&'s self) -> Callback<'s, O> {
        Callback::new(self, &self.output)
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
    vec: Vec<Callback<'static, O>>,
    run_counter: Option<u64>,
}

impl<O: 'static> Default for Callbacks<O> {
    fn default() -> Self {
        Self {
            vec: Default::default(),
            run_counter: None,
        }
    }
}

impl<O: Clone> Callbacks<O> {
    pub fn add<'s>(&mut self, callback: Callback<'s, O>, run_count: u64) {
        let old_run_count = self.run_counter.replace(run_count).unwrap_or(run_count);
        if old_run_count != run_count {
            panic!(
                "Stale callbacks found from previous run #{old_run_count}, now on run #{run_count}"
            );
        }
        self.vec
            .push(unsafe { transmute::<Callback<'s, O>, Callback<'static, O>>(callback) });
    }

    pub fn run(self, ctx: Ctx, mut value_factory: impl FnMut() -> Cached<O>) {
        if self.vec.is_empty() {
            return;
        }

        let run_count = ctx.run_count;

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
            ctx.scope
                .spawn(move |scope| downstream.run(Ctx { scope, run_count }))
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
    fn request<'s>(&self, ctx: Ctx<'_, 's>, counter: &AtomicU32, downstream: &'s dyn Downstream)
    where
        Self: 's;
    fn get(&self) -> Cached<Self::Combined>;
}

impl UpstreamCollectorExt<()> for UpstreamCollector<(), ()> {
    type Combined = ();

    fn new(upstreams: ()) -> Self {
        UpstreamCollector {
            upstreams,
            outputs: (),
        }
    }

    fn request<'s>(&self, ctx: Ctx<'_, 's>, _counter: &AtomicU32, downstream: &'s dyn Downstream)
    where
        Self: 's,
    {
        downstream.run(ctx);
    }

    fn get(&self) -> Cached<()> {
        Cached::Constant(())
    }
}

macro_rules! impl_upstream_collector_tuple {
    ($($t:ident $u:ident $c:ident),*) => {
        impl<$($t: Upstream),*> UpstreamCollectorExt<($($t,)*),> for UpstreamCollector<($($t,)*), ($(AtomicCell<Option<Cached<$t::Output>>>,)*)>
        where $($t::Output: Clone),* {
            type Combined = ($($t::Output,)*);

            fn new(upstreams: ($($t,)*)) -> Self {
                Self {
                    upstreams,
                    outputs: Default::default()
                }
            }

            #[allow(unused)]
            fn request<'s>(&self, ctx: Ctx<'_, 's>, counter: &AtomicU32, downstream: &'s dyn Downstream) where Self: 's {
                let mut count = $( one::<$t>() +)* 0;
                counter.store(count, Ordering::Relaxed);

                let ($($u,)*) = &self.upstreams;
                let ($($c,)*) = &self.outputs;

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

impl<U: Upstream<Output = O>, O: Send + Clone + 'static> UpstreamCollectorExt<Vec<U>>
    for UpstreamCollector<Vec<U>, Vec<AtomicCell<Option<Cached<O>>>>>
{
    type Combined = Vec<O>;

    fn new(upstreams: Vec<U>) -> Self {
        let mut outputs = Vec::with_capacity(upstreams.len());
        for _ in 0..upstreams.len() {
            outputs.push(AtomicCell::new(None));
        }
        UpstreamCollector { upstreams, outputs }
    }

    fn request<'s>(&self, ctx: Ctx<'_, 's>, counter: &AtomicU32, downstream: &'s dyn Downstream)
    where
        Self: 's,
    {
        if self.upstreams.is_empty() {
            if downstream.should_run() {
                downstream.run(ctx);
            }
            return;
        }

        counter.store(self.upstreams.len() as u32, Ordering::Relaxed);

        let mut iter = self.upstreams.iter().enumerate();
        let (_, first) = iter.next().unwrap();

        let run_count = ctx.run_count;

        for (i, upstream) in iter {
            let callback = unsafe {
                Callback::new(
                    downstream,
                    transmute::<&AtomicCell<Option<Cached<_>>>, &'s AtomicCell<Option<Cached<_>>>>(
                        &self.outputs[i],
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
                    &self.outputs[0],
                ),
            )
        });
    }

    fn get(&self) -> Cached<Vec<O>> {
        let mut combined_senders = vec![];
        let mut constant = true;
        let mut revalidate = true;
        let result = self
            .outputs
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
                        constant = false;
                        revalidate = revalidate && *revalidated;
                        combined_senders.append(senders);
                        value.clone()
                    }
                };
                c.store(Some(cached));
                result
            })
            .collect();

        if constant {
            Cached::Constant(result)
        } else {
            Cached::Variable {
                value: result,
                senders: combined_senders,
                revalidated: revalidate,
            }
        }
    }
}

impl<const N: usize, U: Upstream<Output = O>, O: Send + Clone + 'static>
    UpstreamCollectorExt<[U; N]> for UpstreamCollector<[U; N], [AtomicCell<Option<Cached<O>>>; N]>
{
    type Combined = [O; N];

    fn new(upstreams: [U; N]) -> Self {
        UpstreamCollector {
            upstreams,
            outputs: array_init(|_| AtomicCell::new(None)),
        }
    }

    fn request<'s>(&self, ctx: Ctx<'_, 's>, counter: &AtomicU32, downstream: &'s dyn Downstream)
    where
        Self: 's,
    {
        if self.upstreams.is_empty() {
            if downstream.should_run() {
                downstream.run(ctx);
            }
            return;
        }

        counter.store(self.upstreams.len() as u32, Ordering::Relaxed);

        let mut iter = self.upstreams.iter().enumerate();
        let (_, first) = iter.next().unwrap();

        let run_count = ctx.run_count;

        for (i, upstream) in iter {
            let callback = unsafe {
                Callback::new(
                    downstream,
                    transmute::<&AtomicCell<Option<Cached<_>>>, &'s AtomicCell<Option<Cached<_>>>>(
                        &self.outputs[i],
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
                    &self.outputs[0],
                ),
            )
        });
    }

    fn get(&self) -> Cached<[O; N]> {
        let mut combined_senders = vec![];
        let mut constant = true;
        let mut revalidate = true;
        let result = array_init(|i| {
            let mut cached = self.outputs[i].take().unwrap();
            let result = match &mut cached {
                Cached::Constant(v) => v.clone(),
                Cached::Variable {
                    value,
                    senders,
                    revalidated,
                } => {
                    constant = false;
                    revalidate = revalidate && *revalidated;
                    combined_senders.append(senders);
                    value.clone()
                }
            };
            self.outputs[i].store(Some(cached));
            result
        });

        if constant {
            Cached::Constant(result)
        } else {
            Cached::Variable {
                value: result,
                senders: combined_senders,
                revalidated: revalidate,
            }
        }
    }
}
