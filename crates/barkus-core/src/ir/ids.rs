use std::ops::Index;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProductionId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SymbolId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PoolId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

impl<T> Index<ProductionId> for Vec<T> {
    type Output = T;
    fn index(&self, id: ProductionId) -> &T {
        &self[id.0 as usize]
    }
}

impl<T> Index<SymbolId> for Vec<T> {
    type Output = T;
    fn index(&self, id: SymbolId) -> &T {
        &self[id.0 as usize]
    }
}

impl<T> Index<NodeId> for Vec<T> {
    type Output = T;
    fn index(&self, id: NodeId) -> &T {
        &self[id.0 as usize]
    }
}
