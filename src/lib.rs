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
    fn into_run(self) -> RunInWorld<R>;
}

pub struct RunInWorld<R> {
    pub run: R,
    pub world: World,
}

impl<R> RunInWorld<R> {
    pub fn new(run: R, world: World) -> Self {
        RunInWorld { run, world }
    }
}

impl<R: Run> IntoRun<R> for RunInWorld<R> {
    fn into_run(self) -> Self {
        self
    }
}

pub fn run<R: Run>(r: impl IntoRun<R>) -> Result<R::Output, IncompatibleWorldErr> {
    static COMPUTE: ThreadPool = ThreadPool::new();

    let RunInWorld {
        run: converted,
        world,
    } = r.into_run();

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
