use std::{ops::Deref, sync::Arc};

use derive_destructure2::destructure;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use petgraph::{acyclic::Acyclic, data::Build, prelude::StableDiGraph, visit::GraphBase};

use crate::{Ctx, Run, cache::MaybeCached};

pub mod auto;
pub mod op;
pub mod stack;
pub mod variable;

lazy_static! {
    static ref GRAPH: Mutex<Acyclic<StableDiGraph<(), ()>>> = Mutex::new(Acyclic::new());
}

pub struct Node<T: ?Sized> {
    arc: Arc<T>,
    id: <StableDiGraph<(), ()> as GraphBase>::NodeId,
}

#[derive(destructure)]
struct EmptyNode {
    id: <StableDiGraph<(), ()> as GraphBase>::NodeId,
}

impl Node<()> {
    fn empty() -> EmptyNode {
        EmptyNode {
            id: GRAPH.lock().add_node(()),
        }
    }
}

impl EmptyNode {
    fn init<T>(self, value: T) -> Node<T> {
        let (id,) = self.destructure();
        Node {
            arc: Arc::new(value),
            id,
        }
    }

    fn add_edge<U: ?Sized>(&self, other: &Node<U>) {
        GRAPH.lock().add_edge(self.id, other.id, ());
    }
}

impl Drop for EmptyNode {
    fn drop(&mut self) {
        unreachable!();
    }
}

impl<T> Node<T> {
    #[allow(unused)]
    fn new(value: T) -> Self {
        let arc = Arc::new(value);
        let id = GRAPH.lock().add_node(());
        Node { arc, id }
    }

    fn add_edge<U: ?Sized>(&self, other: &Node<U>) {
        GRAPH.lock().add_edge(self.id, other.id, ());
    }

    fn remove_edge<U: ?Sized>(&self, other: &Node<U>) {
        let mut lock = GRAPH.lock();
        let edge = lock
            .find_edge(self.id, other.id)
            .expect("Cannot remove edge between nodes that were not connected");
        lock.remove_edge(edge);
    }
}

impl<T: Run + 'static> Node<T> {
    fn new_dyn(value: T) -> Node<dyn Run<Output = T::Output>> {
        let arc = Arc::new(value);
        let id = GRAPH.lock().add_node(());
        Node { arc, id }
    }
}

impl<T: ?Sized> Clone for Node<T> {
    fn clone(&self) -> Self {
        Node {
            arc: self.arc.clone(),
            id: self.id,
        }
    }
}

impl<T: ?Sized> Drop for Node<T> {
    fn drop(&mut self) {
        if Arc::strong_count(&self.arc) == 1 {
            if cfg!(debug_assertions) {
                assert_eq!(
                    GRAPH
                        .lock()
                        .edges_directed(self.id, petgraph::Direction::Incoming)
                        .count(),
                    0
                );
            }
            println!("{:?}", GRAPH.lock().nodes_iter().collect::<Vec<_>>());
            GRAPH.lock().remove_node(self.id).unwrap_or_else(|| {
                panic!(
                    "Node was not found in graph when dropping id: {:?}",
                    self.id
                )
            });
        }
    }
}

impl<T> Deref for Node<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.arc
    }
}

impl<T: ?Sized + Run> Run for Node<T> {
    type Output = T::Output;

    fn run(&self, ctx: Ctx) -> MaybeCached<T::Output> {
        self.arc.run(ctx)
    }
}

#[cfg(test)]
mod tests {
    use petgraph::data::DataMap;

    use crate::graph::GRAPH;

    use super::Node;

    #[test]
    #[should_panic]
    fn panic_on_cycle() {
        let a = Node::new(());
        let b = Node::new(());

        a.add_edge(&b);
        b.add_edge(&a);
    }

    #[test]
    fn remove_from_graph() {
        let id = {
            let b = Node::new(());
            let a = Node::new(());

            a.add_edge(&b);

            assert_eq!(GRAPH.lock().node_weight(b.id), Some(&()));

            b.id
        };

        assert_eq!(GRAPH.lock().node_weight(id), None);
    }
}
