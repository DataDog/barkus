use crate::ir::ids::{NodeId, ProductionId};

#[derive(Debug, Clone)]
pub struct TapeMap {
    pub entries: Vec<TapeEntry>,
    pub modifier_bytes: Vec<ModifierByte>,
}

#[derive(Debug, Clone, Copy)]
pub struct TapeEntry {
    pub tape_offset: usize,
    pub tape_len: usize,
    pub node_id: NodeId,
    pub production_id: ProductionId,
}

/// Records the tape position of a modifier byte (Optional choice or Repetition count).
#[derive(Debug, Clone, Copy)]
pub enum ModifierByte {
    Optional {
        tape_offset: usize,
    },
    Repetition {
        tape_offset: usize,
        min: u32,
        max: u32,
    },
}

impl TapeMap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            modifier_bytes: Vec::new(),
        }
    }

    pub fn with_capacity(n: usize) -> Self {
        Self {
            entries: Vec::with_capacity(n),
            modifier_bytes: Vec::new(),
        }
    }

    pub fn push(
        &mut self,
        tape_offset: usize,
        tape_len: usize,
        node_id: NodeId,
        production_id: ProductionId,
    ) {
        self.entries.push(TapeEntry {
            tape_offset,
            tape_len,
            node_id,
            production_id,
        });
    }

    pub fn push_optional(&mut self, tape_offset: usize) {
        self.modifier_bytes
            .push(ModifierByte::Optional { tape_offset });
    }

    pub fn push_repetition(&mut self, tape_offset: usize, min: u32, max: u32) {
        self.modifier_bytes.push(ModifierByte::Repetition {
            tape_offset,
            min,
            max,
        });
    }
}

impl Default for TapeMap {
    fn default() -> Self {
        Self::new()
    }
}
