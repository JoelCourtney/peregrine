pub mod cache;
pub mod data;
pub mod macro_prelude;
pub mod node;
pub mod plan;
pub mod structure;

pub use peregrine_macros::node;

use cache::MaybeCached;
use forte::{ThreadPool, Worker};

pub trait Node: Send + Sync {
    type Output: Send + 'static;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output>;
}

type DynNode<O> = Box<dyn Node<Output = O>>;

pub trait IntoNode<N: Node> {
    fn into_node(self) -> N;
}

pub trait Init {
    type Value;

    fn init(value: Self::Value) -> Self;
}

pub fn run<N: Node>(node: impl IntoNode<N>) -> N::Output {
    static COMPUTE: ThreadPool = ThreadPool::new();
    COMPUTE.resize_to_available();

    COMPUTE.with_worker(|w| node.into_node().run(w)).open()
}
