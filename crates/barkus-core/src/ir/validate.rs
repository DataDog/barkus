use crate::error::IrError;

use super::grammar::{GrammarIr, Symbol};

impl GrammarIr {
    pub fn validate(&self) -> Result<(), IrError> {
        // Check start production exists.
        if self.start.0 as usize >= self.productions.len() {
            return Err(IrError::MissingStartProduction);
        }

        for prod in &self.productions {
            // No empty alternatives.
            if prod.alternatives.is_empty() {
                return Err(IrError::EmptyAlternative {
                    production: prod.id,
                });
            }

            for alt in &prod.alternatives {
                if alt.symbols.is_empty() {
                    return Err(IrError::EmptyAlternative {
                        production: prod.id,
                    });
                }

                for sym_ref in &alt.symbols {
                    // All SymbolRefs point to valid SymbolIds.
                    if sym_ref.symbol.0 as usize >= self.symbols.len() {
                        return Err(IrError::InvalidSymbolRef(sym_ref.symbol));
                    }

                    // If symbol is a NonTerminal, check ProductionId is valid.
                    if let Symbol::NonTerminal(pid) = &self.symbols[sym_ref.symbol] {
                        if pid.0 as usize >= self.productions.len() {
                            return Err(IrError::InvalidProductionRef(*pid));
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
