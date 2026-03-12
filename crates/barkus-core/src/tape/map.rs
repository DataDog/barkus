use crate::ir::ids::NodeId;

#[derive(Debug, Clone)]
pub struct TapeMap {
    pub entries: Vec<TapeEntry>,
}

#[derive(Debug, Clone, Copy)]
pub struct TapeEntry {
    pub tape_offset: usize,
    pub tape_len: usize,
    pub node_id: NodeId,
}

impl TapeMap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, tape_offset: usize, tape_len: usize, node_id: NodeId) {
        self.entries.push(TapeEntry {
            tape_offset,
            tape_len,
            node_id,
        });
    }
}

impl Default for TapeMap {
    fn default() -> Self {
        Self::new()
    }
}
