pub mod cache;
pub mod data;
pub mod macro_prelude;
pub mod node;
pub(crate) mod once_bool;
pub mod world;

pub use peregrine_macros::op;

use cache::MaybeCached;
use forte::{ThreadPool, Worker};

use crate::world::{World, WorldId, WorldView};

pub trait Run: Send + Sync {
    type Output: Send;

    fn world_id(&self) -> WorldId;
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

pub fn run<R: Run>(world: &World, r: impl IntoRun<R>) -> Result<R::Output, IncompatibleWorldErr> {
    static COMPUTE: ThreadPool = ThreadPool::new();

    let converted = r.into_run();
    if !converted.world_id().matches(&world.id()) {
        return Err(IncompatibleWorldErr);
    }

    COMPUTE.resize_to_available();

    let result = COMPUTE
        .with_worker(|w| {
            let ctx = unsafe {
                Ctx {
                    worker: w,
                    world: world.view(),
                }
            };
            converted.run(ctx)
        })
        .open();

    Ok(result)
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub struct IncompatibleWorldErr;
