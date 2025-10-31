pub mod cache;
pub mod data;
pub mod macro_prelude;
pub mod node;
pub(crate) mod once_bool;
pub mod world;

pub use peregrine_macros::op;

use cache::MaybeCached;
use forte::{ThreadPool, Worker};

use crate::world::{World, WorldView};

pub trait Run: Send + Sync {
    type Output: Send;

    fn run(&self, w: Ctx) -> MaybeCached<Self::Output>;
}

#[derive(Copy, Clone)]
pub struct Ctx<'a> {
    pub worker: &'a Worker,
    pub world: WorldView<'a>,
}

pub trait IntoRun<R: Run> {
    fn into_run(self) -> R;
}

pub fn run<R: Run>(world: &World, node: impl IntoRun<R>) -> R::Output {
    static COMPUTE: ThreadPool = ThreadPool::new();
    COMPUTE.resize_to_available();

    COMPUTE
        .with_worker(|w| {
            let ctx = unsafe {
                Ctx {
                    worker: w,
                    world: world.view(),
                }
            };
            node.into_run().run(ctx)
        })
        .open()
}
