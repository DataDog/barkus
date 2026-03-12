use crate::ir::ids::{NodeId, ProductionId};

#[derive(Debug, Clone)]
pub struct Ast {
    pub nodes: Vec<AstNode>,
    pub root: NodeId,
}

#[derive(Debug, Clone)]
pub struct AstNode {
    pub id: NodeId,
    pub kind: AstNodeKind,
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone)]
pub enum AstNodeKind {
    Production(ProductionId),
    Terminal(Vec<u8>),
}

impl Ast {
    pub fn new_node(&mut self, kind: AstNodeKind) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(AstNode {
            id,
            kind,
            children: Vec::new(),
        });
        id
    }

    pub fn add_child(&mut self, parent: NodeId, child: NodeId) {
        self.nodes[parent.0 as usize].children.push(child);
    }

    /// Flatten AST to bytes by walking terminal nodes in order.
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.serialize_node(self.root, &mut out);
        out
    }

    fn serialize_node(&self, node_id: NodeId, out: &mut Vec<u8>) {
        let node = &self.nodes[node_id];
        match &node.kind {
            AstNodeKind::Terminal(bytes) => out.extend_from_slice(bytes),
            AstNodeKind::Production(_) => {
                for &child in &node.children {
                    self.serialize_node(child, out);
                }
            }
        }
    }
}
