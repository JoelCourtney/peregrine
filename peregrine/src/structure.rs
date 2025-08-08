use crate::Node;

trait ToNode {
    type Output;
    
    fn to_node(&self) -> impl Node<Output=Self::Output>;
}

impl<N: Node> ToNode for N {
    type Output = N::Output;

    fn to_node(&self) -> impl Node<Output=Self::Output> {
        self
    }
}
