use std::{mem::transmute, sync::atomic::AtomicU32};

use crate::{
    Callback, Ctx, Downstream,
    cache::{Cached, collector::OutputCell},
};

pub(crate) struct Sink<O> {
    output: OutputCell<O>,
    _counter: AtomicU32,
}

impl<O: Clone + Send + 'static> Sink<O> {
    pub(crate) fn new() -> Self {
        Self {
            output: OutputCell::default(),
            _counter: AtomicU32::new(0),
        }
    }

    pub(crate) fn as_callback<'s>(&'s self) -> Callback<'s, O> {
        Callback::new(self, &self.output)
    }

    pub(crate) fn open(self) -> O {
        self.output.take().unwrap().open()
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
