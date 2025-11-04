pub mod cache;
pub mod data;
pub mod macro_prelude;
pub mod node;
pub(crate) mod once_bool;

pub use peregrine_macros::op;

use cache::MaybeCached;
use forte::{ThreadPool, Worker};

pub trait Run: Send + Sync {
    type Output: Send;

    fn run(&self, w: Ctx) -> MaybeCached<Self::Output>;
}

#[derive(Copy, Clone)]
pub struct Ctx<'a> {
    pub worker: &'a Worker,
}

pub trait IntoRun<R: Run> {
    fn into_run(self) -> R;
}

pub fn run<R: Run>(r: impl IntoRun<R>) -> R::Output {
    static COMPUTE: ThreadPool = ThreadPool::new();

    COMPUTE.resize_to_available();

    COMPUTE
        .with_worker(|w| {
            let ctx = Ctx { worker: w };
            r.into_run().run(ctx)
        })
        .open()
}
