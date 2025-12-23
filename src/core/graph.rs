// nodes, edges, IR 
use std::collections::{HashMap, HashSet};
use std::fmt::{self, write};
use crate::core::types::{NodeId, EdgeId, Counter, SubgraphKind, EdgeKind, NodeKind};
use crate::core::state::EdgeState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphError {
    ParentNotFound(NodeId),
    NodeNotFound(NodeId),
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GraphError::ParentNotFound(id) => write!(f, "Parent node not found: {}", id),
            GraphError::NodeNotFound(id) => write!(f, "Node not found: {}", id),
        }
    }
}

impl std::error::Error for GraphError{} 

pub struct Node {
    id: NodeId,
    name: String,
    subgraph: SubgraphKind,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
}

pub struct Edge {
    id: EdgeId,
    from: NodeId,
    to: NodeId,
    kind: EdgeKind,
    subgraph: SubgraphKind,
    state: EdgeState,
    counter: Counter,
}

pub struct ReflexionGraph {
    nodes: HashMap<NodeId, Node>,
    edges: HashMap<EdgeId, Edge>,
    impl_out: HashMap<NodeId, Vec<EdgeId>>,
    arch_out: HashMap<NodeId, Vec<EdgeId>>,
    maps_to: HashMap<NodeId, NodeId>,
    propagation_table: HashMap<EdgeId, HashSet<EdgeId>>, //arc/propagated edge -> impl edges
    next_node_id: NodeId,
    next_edge_id: EdgeId,
}

impl ReflexionGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            impl_out: HashMap::new(),
            arch_out: HashMap::new(),
            maps_to: HashMap::new(),
            propagation_table: HashMap::new(), //arc/propagated edge -> impl edges
            next_node_id: 1, 
            next_edge_id: 1,
        }
    }

    pub fn fresh_node_id(&mut self) -> NodeId {
        let id = self.next_node_id;
        self.next_node_id += 1;
        id
    }

    pub fn fresh_edge_id(&mut self) -> EdgeId {
        let id = self.next_edge_id;
        self.next_edge_id += 1;
        id
    }

    pub fn add_node(&mut self, mut node: Node) -> Result<NodeId, GraphError> {
        //if parent is specified, it must already exist
        if let Some(parent_id) = node.parent && !self.nodes.contains_key(&parent_id) {
            return Err(GraphError::ParentNotFound(parent_id));
        }

        //now graph owns identity, assign fresh IDs
        let id = self.fresh_node_id();
        node.id = id;

        //insert node
        self.nodes.insert(id,node);

        //update parent's children list if needed 
        if let Some(parent_id) = self.nodes.get(&id).and_then(|n| n.parent) {
            self.nodes.get_mut(&parent_id).expect("Checked Above").children.push(id);
        }

        Ok(id)
    }

    pub fn add_edge(&mut self, mut edge: Edge) -> Result<EdgeId, GraphError> {
        //validate that there is a source and destination (from/to edges)
        if !self.nodes.contains_key(&edge.from) {
            return Err(GraphError::NodeNotFound(edge.from));
        }

        if !self.nodes.contains_key(&edge.to) {
            return Err(GraphError::NodeNotFound(edge.to));
        }

        //now graph owns identity, assign fresh IDs
        let id = self.fresh_edge_id();
        edge.id = id;

        //insert edge
        self.edges.insert(id, edge);

        //update adjacency list based on subgraph
        let edge_ref = self.edges.get(&id).expect("Just Inserted");

        match edge_ref.subgraph {
            SubgraphKind::Implementation => {
                self.impl_out.entry(edge_ref.from).or_default().push(id);
            } 
            SubgraphKind::Architecture | SubgraphKind::Propagated => {
                self.arch_out.entry(edge_ref.from).or_default().push(id);
            }
        }

        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{EdgeKind, SubgraphKind};
    use crate::core::state::EdgeState;

    fn mk_node(name: &str, subgraph: SubgraphKind, parent: Option<NodeId>) -> Node {
        Node {
            id: 0, // will be overwritten by add_node
            name: name.to_string(),
            subgraph,
            parent,
            children: vec![],
        }
    }

    fn mk_edge(from: NodeId, to: NodeId, subgraph: SubgraphKind, kind: EdgeKind) -> Edge {
        Edge {
            id: 0, // will be overwritten by add_edge
            from,
            to,
            kind,
            subgraph,
            state: EdgeState::Undefined,
            counter: 0,
        }
    }

    #[test]
    fn add_node_sets_id_and_updates_parent_children() {
        let mut g = ReflexionGraph::new();

        let parent_id = g.add_node(mk_node("A", SubgraphKind::Architecture, None)).unwrap();
        let child_id = g
            .add_node(mk_node("B", SubgraphKind::Architecture, Some(parent_id)))
            .unwrap();

        // parent exists and has the child
        let parent = g.nodes.get(&parent_id).unwrap();
        assert!(parent.children.contains(&child_id));

        // child points to parent
        let child = g.nodes.get(&child_id).unwrap();
        assert_eq!(child.parent, Some(parent_id));
    }

    #[test]
    fn add_node_rejects_missing_parent() {
        let mut g = ReflexionGraph::new();

        let err = g
            .add_node(mk_node("B", SubgraphKind::Architecture, Some(999)))
            .unwrap_err();

        assert_eq!(err, GraphError::ParentNotFound(999));
    }

    #[test]
    fn add_edge_updates_correct_adjacency_map() {
        let mut g = ReflexionGraph::new();

        // 2 arch nodes, 2 impl nodes
        let a1 = g.add_node(mk_node("Arch1", SubgraphKind::Architecture, None)).unwrap();
        let a2 = g.add_node(mk_node("Arch2", SubgraphKind::Architecture, None)).unwrap();
        let i1 = g.add_node(mk_node("Impl1", SubgraphKind::Implementation, None)).unwrap();
        let i2 = g.add_node(mk_node("Impl2", SubgraphKind::Implementation, None)).unwrap();

        // 1 arch edge, 1 impl edge
        let e_arch = g
            .add_edge(mk_edge(a1, a2, SubgraphKind::Architecture, EdgeKind::contains()))
            .unwrap();

        let e_impl = g
            .add_edge(mk_edge(i1, i2, SubgraphKind::Implementation, EdgeKind::calls()))
            .unwrap();

        // arch_out contains e_arch at a1
        let arch_out = g.arch_out.get(&a1).unwrap();
        assert!(arch_out.contains(&e_arch));

        // impl_out contains e_impl at i1
        let impl_out = g.impl_out.get(&i1).unwrap();
        assert!(impl_out.contains(&e_impl));
    }

    #[test]
    fn add_edge_rejects_missing_nodes() {
        let mut g = ReflexionGraph::new();

        let n1 = g.add_node(mk_node("N1", SubgraphKind::Architecture, None)).unwrap();

        // to node missing
        let err = g
            .add_edge(mk_edge(n1, 999, SubgraphKind::Architecture, EdgeKind::depends_on()))
            .unwrap_err();

        assert_eq!(err, GraphError::NodeNotFound(999));
    }
}
