use std::fmt;

use crate::ir::ids::{ProductionId, SymbolId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerateError {
    BudgetExhausted { kind: BudgetKind },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetKind {
    MaxDepth,
    MaxTotalNodes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrError {
    InvalidProductionRef(ProductionId),
    InvalidSymbolRef(SymbolId),
    EmptyAlternative { production: ProductionId },
    MissingStartProduction,
    MinDepthInconsistency { production: ProductionId },
}

impl fmt::Display for GenerateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenerateError::BudgetExhausted { kind } => {
                write!(f, "budget exhausted: {kind}")
            }
        }
    }
}

impl fmt::Display for BudgetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BudgetKind::MaxDepth => write!(f, "max depth"),
            BudgetKind::MaxTotalNodes => write!(f, "max total nodes"),
        }
    }
}

impl fmt::Display for IrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IrError::InvalidProductionRef(id) => {
                write!(f, "invalid production reference: {}", id.0)
            }
            IrError::InvalidSymbolRef(id) => {
                write!(f, "invalid symbol reference: {}", id.0)
            }
            IrError::EmptyAlternative { production } => {
                write!(f, "empty alternative in production {}", production.0)
            }
            IrError::MissingStartProduction => write!(f, "missing start production"),
            IrError::MinDepthInconsistency { production } => {
                write!(f, "min depth inconsistency in production {}", production.0)
            }
        }
    }
}

impl std::error::Error for GenerateError {}
impl std::error::Error for IrError {}
