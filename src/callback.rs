use std::{any::TypeId, mem::transmute};

use crate::{
    Ctx, Downstream,
    cache::{Cached, collector::OutputCell},
};

pub struct Callback<'s, I> {
    pub downstream: &'s dyn Downstream,
    pub output: Box<dyn CallbackOutput<I> + 's>,
}

pub struct CallbackId(pub usize);

impl<'s, I> Callback<'s, I> {
    pub fn new(downstream: &'s dyn Downstream, output: impl CallbackOutput<I> + 's) -> Self {
        Callback {
            downstream,
            output: Box::new(output),
        }
    }
}

pub trait CallbackOutput<O>: Send + Sync {
    fn store(self: Box<Self>, value: Cached<O>);
}

impl<O: Send + 'static> CallbackOutput<O> for &OutputCell<O> {
    fn store(self: Box<Self>, value: Cached<O>) {
        (*self).store(value);
    }
}

struct CallbackMap<'s, I, O: 'static> {
    modification: Box<dyn FnOnce(Cached<I>) -> Cached<O> + Send + Sync>,
    and_then: Box<dyn CallbackOutput<O> + 's>,
}

impl<I, O: 'static> CallbackOutput<I> for CallbackMap<'_, I, O> {
    fn store(self: Box<Self>, value: Cached<I>) {
        let mapped = (self.modification)(value);
        self.and_then.store(mapped);
    }
}

impl<'s, O: 'static> Callback<'s, O> {
    pub fn call(self, value: Cached<O>, ctx: Ctx) {
        self.output.store(value);
        if self.downstream.should_run() {
            self.downstream.run(ctx);
        }
    }

    pub fn map<I: 's>(
        self,
        func: impl FnOnce(Cached<I>) -> Cached<O> + Send + Sync + 'static,
    ) -> Callback<'s, I> {
        Callback {
            output: Box::new(CallbackMap {
                modification: Box::new(func),
                and_then: self.output,
            }),
            downstream: self.downstream,
        }
    }
}

pub struct ErasedCallback<'s> {
    callback: Callback<'s, ()>,
    output_type_id: TypeId,
}

impl<'s> ErasedCallback<'s> {
    pub fn new<O: 'static>(callback: Callback<'s, O>) -> Self {
        // SAFETY: The typeid of O is stored, the callback can only be accessed
        // if opened with the same type
        let erased = unsafe { transmute::<Callback<'s, O>, Callback<'s, ()>>(callback) };
        ErasedCallback {
            callback: erased,
            output_type_id: TypeId::of::<O>(),
        }
    }

    pub fn open<O: 'static>(self) -> Callback<'s, O> {
        assert_eq!(TypeId::of::<O>(), self.output_type_id);
        // SAFETY: The typeid of O is stored, the callback can only be accessed
        // if opened with the same type
        unsafe { transmute::<Callback<'s, ()>, Callback<'s, O>>(self.callback) }
    }
}

pub(crate) struct Sink<O> {
    output: OutputCell<O>,
}

impl<O: Clone + Send + 'static> Sink<O> {
    pub(crate) fn new() -> Self {
        Self {
            output: OutputCell::default(),
        }
    }

    pub(crate) fn as_callback(&self) -> Callback<'_, O> {
        Callback::new(self, &self.output)
    }

    pub(crate) fn open(self) -> O {
        self.output
            .take()
            .expect("todo: An output was not present. This is an internal error")
            .open()
    }
}

impl<O: Send> Downstream for Sink<O> {
    fn should_run(&self) -> bool {
        false
    }

    fn run(&self, _: Ctx) {}
}
