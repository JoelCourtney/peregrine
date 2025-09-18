/*
* List of node structures:
* - Stack
*   - evaluates to last element
*   - elements can only access previous element
*   - nodes only inserted at end
* - Accumulator
    - Uses the Accumulate trait
    - Only accepts Accumulate::Diff nodes
    - evaluates to the accumulated value
* - Sequence
    - evaluates to ordered list of element results
    - accepts nodes that eval to a given type
    - can be queried internally and externally at any index
* - Stage
    - evaluates to last element
    - elements can only access previous element
    - nodes inserted at any unique index
* - ParStage
    - evaluates to last element
    - elements can only access previous element
    - nodes inserted at any index
    - nodes produce Accumulate::Diff
*/

pub mod stack;

use forte::Worker;
use parking_lot::Mutex;

use crate::{
    IntoNode, Node,
    cache::{Cache, MaybeCached},
    node::NodeBox,
};

pub struct NodeCell<O>(Mutex<NodeBox<O>>, Cache<()>);

impl<O> NodeCell<O> {
    pub fn new<N: Node<Output = O> + 'static>(node: impl IntoNode<N>) -> Self {
        NodeCell(Mutex::new(NodeBox::new(node.into_node())), Cache::new())
    }

    pub fn set<N: Node<Output = O> + 'static>(&self, node: impl IntoNode<N>) {
        let mut lock = self.0.lock();
        *lock = NodeBox::new(node.into_node());
        self.1.invalidate();
    }
}

impl<O: Clone + Send + 'static> Node for NodeCell<O> {
    type Output = O;

    fn run(&self, w: &Worker) -> MaybeCached<Self::Output> {
        let mut result = self.0.lock().run(w);
        result.push_sender(self.1.get_invalidation_sender());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as peregrine;
    use crate::structure::stack::Stack;
    use crate::*;

    #[test]
    fn test_cell() {
        let cell = NodeCell::new(0);
        assert_eq!(run(&cell), 0);

        cell.set({
            let stack = Stack::new(2);
            stack.push(|prev| op!(i!(prev) * 3));
            stack
        });

        assert_eq!(run(&cell), 6);
    }
}
