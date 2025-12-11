use lazy_static::lazy_static;
use petgraph::{
    acyclic::{Acyclic, AcyclicEdgeError},
    data::Build,
    prelude::StableDiGraph,
    visit::GraphBase,
};
use std::sync::Mutex;

pub mod auto;
pub mod many;
pub mod op;
pub mod series;
pub mod stack;
pub mod variable;

lazy_static! {
    pub(crate) static ref GRAPH: Mutex<Acyclic<StableDiGraph<(), ()>>> = Mutex::new(Acyclic::new());
}

pub struct NodeTracker {
    id: NodeId,
}

pub type NodeId = <StableDiGraph<(), ()> as GraphBase>::NodeId;

impl NodeTracker {
    fn new() -> NodeTracker {
        NodeTracker {
            id: GRAPH.lock().unwrap().add_node(()),
        }
    }

    fn add_edges(&self, other: impl IntoIterator<Item = NodeId>) {
        let mut lock = GRAPH.lock().unwrap();
        let err = other.into_iter().try_for_each(|o| {
            lock.try_add_edge(self.id, o, ())?;
            Ok::<_, AcyclicEdgeError<_>>(())
        });
        drop(lock);
        err.expect("Cycle detected in dependency graph.");
    }

    fn remove_edges(&self, other: impl IntoIterator<Item = NodeId>) {
        let mut lock = GRAPH.lock().unwrap();
        let err = other.into_iter().try_for_each(|o| {
            let edge = lock.find_edge(self.id, o).ok_or(())?;
            lock.remove_edge(edge);
            Ok::<_, ()>(())
        });
        drop(lock);
        err.expect("Cannot remove edge between nodes that were not connected.");
    }
}

impl Drop for NodeTracker {
    fn drop(&mut self) {
        let mut lock = GRAPH.lock().unwrap();
        if cfg!(debug_assertions) && !std::thread::panicking() {
            let incoming = lock
                .edges_directed(self.id, petgraph::Direction::Incoming)
                .count();
            if incoming != 0 {
                // Dropping the lock and reacquiring it inside the panic turns it
                // into a temporary that will be dropped before stack unwinding begins.
                drop(lock);
                panic!(
                    "Tried to remove a node that still has references pointing to it: {:?} <- {:?}",
                    self.id,
                    GRAPH
                        .lock()
                        .unwrap()
                        .edges_directed(self.id, petgraph::Direction::Incoming)
                        .collect::<Vec<_>>()
                );
            }
        }
        let err = lock.remove_node(self.id);
        drop(lock);
        err.unwrap_or_else(|| {
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

    use super::NodeTracker;

    #[test]
    #[should_panic]
    fn panic_on_cycle() {
        let a = NodeTracker::new();
        let b = NodeTracker::new();

        a.add_edges([b.id]);
        b.add_edges([a.id]);
    }

    #[test]
    fn add_to_graph() {
        let b = NodeTracker::new();
        let a = NodeTracker::new();

        a.add_edges([b.id]);

        assert_eq!(GRAPH.lock().unwrap().node_weight(a.id), Some(&()));
        assert_eq!(GRAPH.lock().unwrap().node_weight(b.id), Some(&()));

        assert!(GRAPH.lock().unwrap().find_edge(a.id, b.id).is_some());
    }
}
