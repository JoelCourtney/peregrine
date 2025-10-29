pub mod cache;
pub mod data;
pub mod macro_prelude;
pub mod node;
pub mod structure;
pub(crate) mod once_bool;

pub use peregrine_macros::op;

use cache::MaybeCached;
use forte::{ThreadPool, Worker};

pub trait Node: Send + Sync {
    type Output: Send + 'static;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output>;
}

pub trait IntoNode<N: Node> {
    fn into_node(self) -> N;
}

pub fn run<N: Node>(node: impl IntoNode<N>) -> N::Output {
    static COMPUTE: ThreadPool = ThreadPool::new();
    COMPUTE.resize_to_available();

    COMPUTE.with_worker(|w| node.into_node().run(w)).open()
}
