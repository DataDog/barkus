use crate::error::IrError;

use super::grammar::{GrammarIr, Modifier, Symbol, SymbolRef, TerminalKind};

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
                    self.validate_symbol_ref(sym_ref)?;
                }
            }
        }

        for pool in &self.token_pools {
            for alt in &pool.alternatives {
                for sym_ref in &alt.symbols {
                    self.validate_symbol_ref(sym_ref)?;
                }
            }
        }

        Ok(())
    }

    fn validate_symbol_ref(&self, sym_ref: &SymbolRef) -> Result<(), IrError> {
        // All SymbolRefs point to valid SymbolIds.
        if sym_ref.symbol.0 as usize >= self.symbols.len() {
            return Err(IrError::InvalidSymbolRef(sym_ref.symbol));
        }

        validate_modifier(&sym_ref.modifier)?;

        match &self.symbols[sym_ref.symbol] {
            Symbol::NonTerminal(pid) => {
                if pid.0 as usize >= self.productions.len() {
                    return Err(IrError::InvalidProductionRef(*pid));
                }
            }
            Symbol::Terminal(tk) => self.validate_terminal(tk)?,
        }

        Ok(())
    }

    fn validate_terminal(&self, tk: &TerminalKind) -> Result<(), IrError> {
        match tk {
            TerminalKind::Literal(_) | TerminalKind::AnyByte => {}
            TerminalKind::ByteRange(lo, hi) => {
                if lo > hi {
                    return Err(IrError::InvalidByteRange(*lo, *hi));
                }
            }
            TerminalKind::CharClass { ranges, .. } => {
                for &(lo, hi) in ranges {
                    if lo > hi {
                        return Err(IrError::InvalidByteRange(lo, hi));
                    }
                }
            }
            TerminalKind::TokenPool(pool_id) => {
                if pool_id.0 as usize >= self.token_pools.len() {
                    return Err(IrError::InvalidPoolRef(*pool_id));
                }
            }
        }
        Ok(())
    }
}

fn validate_modifier(modifier: &Modifier) -> Result<(), IrError> {
    match modifier {
        Modifier::Once | Modifier::Optional => Ok(()),
        Modifier::ZeroOrMore { min, max } => {
            if min > max {
                return Err(IrError::InvalidModifierBounds {
                    min: *min,
                    max: *max,
                });
            }
            Ok(())
        }
        Modifier::OneOrMore { min, max } => {
            if *min < 1 || min > max {
                return Err(IrError::InvalidModifierBounds {
                    min: *min,
                    max: *max,
                });
            }
            Ok(())
        }
    }
}
