use lazy_static::lazy_static;
use parking_lot::Mutex;
use petgraph::{acyclic::Acyclic, data::Build, prelude::StableDiGraph, visit::GraphBase};

pub mod auto;
pub mod op;
pub mod series;
pub mod stack;
pub mod tuple;
pub mod variable;

lazy_static! {
    pub(crate) static ref GRAPH: Mutex<Acyclic<StableDiGraph<(), ()>>> = Mutex::new(Acyclic::new());
}

pub struct Node {
    id: NodeId,
}

pub type NodeId = <StableDiGraph<(), ()> as GraphBase>::NodeId;

impl Node {
    fn new() -> Node {
        Node {
            id: GRAPH.lock().add_node(()),
        }
    }

    fn add_edges(&self, other: impl IntoIterator<Item = NodeId>) {
        let mut lock = GRAPH.lock();
        other.into_iter().for_each(|o| {
            lock.try_add_edge(self.id, o, ())
                .expect("Cycle detected in dependency graph");
        });
    }

    fn remove_edges(&self, other: impl IntoIterator<Item = NodeId>) {
        let mut lock = GRAPH.lock();
        other.into_iter().for_each(|o| {
            let edge = lock
                .find_edge(self.id, o)
                .expect("Cannot remove edge between nodes that were not connected.");
            lock.remove_edge(edge);
        });
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        let mut lock = GRAPH.lock();
        if cfg!(debug_assertions) && !std::thread::panicking() {
            assert_eq!(
                lock.edges_directed(self.id, petgraph::Direction::Incoming)
                    .count(),
                0,
                "Tried to remove a node that still has references pointing to it: {:?} <- {:?}",
                self.id,
                lock.edges_directed(self.id, petgraph::Direction::Incoming)
                    .collect::<Vec<_>>()
            );
        }
        lock.remove_node(self.id).unwrap_or_else(|| {
            panic!(
                "Node was not found in graph when dropping id: {:?}",
                self.id
            )
        });
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
        let a = Node::new();
        let b = Node::new();

        a.add_edges([b.id]);
        b.add_edges([a.id]);
    }

    #[test]
    fn add_to_graph() {
        let b = Node::new();
        let a = Node::new();

        a.add_edges([b.id]);

        assert_eq!(GRAPH.lock().node_weight(a.id), Some(&()));
        assert_eq!(GRAPH.lock().node_weight(b.id), Some(&()));

        assert!(GRAPH.lock().find_edge(a.id, b.id).is_some());
    }
}
