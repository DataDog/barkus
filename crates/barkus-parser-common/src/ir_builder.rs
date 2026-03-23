use std::collections::HashMap;

use barkus_core::ir::analysis::{compute_min_depths, mark_recursive};
use barkus_core::ir::{
    Alternative, GrammarIr, Modifier, Production, ProductionAttrs, ProductionId, Symbol, SymbolId,
    SymbolRef, TerminalKind,
};

use crate::error::ParseError;
use crate::raw::{RawAlternative, RawGrammar, RawQuantifier, RawRule};

/// Trait implemented by each parser crate for its own `RawItem` type.
/// The single method dispatches item-level IR building to format-specific logic.
pub trait BuildItem: Sized {
    fn build_item(
        builder: &mut IrBuilder,
        item: &Self,
        refs: &mut Vec<SymbolRef>,
    ) -> Result<(), ParseError>;
}

pub struct IrBuilder<'a> {
    pub symbols: &'a mut Vec<Symbol>,
    pub name_to_id: &'a HashMap<String, ProductionId>,
    pub anon_productions: &'a mut Vec<Production>,
    pub n_named: u32,
}

impl<'a> IrBuilder<'a> {
    pub fn alloc_symbol(&mut self, sym: Symbol) -> SymbolId {
        let id = SymbolId(self.symbols.len() as u32);
        self.symbols.push(sym);
        id
    }

    pub fn next_anon_id(&self) -> ProductionId {
        ProductionId(self.n_named + self.anon_productions.len() as u32)
    }

    /// Create an anonymous production with the given alternatives and return
    /// a SymbolId referencing it.
    fn push_anon_production(&mut self, alternatives: Vec<Alternative>) -> SymbolId {
        let anon_id = self.next_anon_id();
        self.anon_productions.push(Production {
            id: anon_id,
            name: format!("__anon_{}", anon_id.0),
            alternatives,
            attrs: ProductionAttrs::default(),
        });
        self.alloc_symbol(Symbol::NonTerminal(anon_id))
    }

    pub fn build_alternatives<I: BuildItem>(
        &mut self,
        alts: &[RawAlternative<I>],
        rule_line: usize,
        rule_col: usize,
    ) -> Result<Vec<Alternative>, ParseError> {
        let mut result = Vec::with_capacity(alts.len());
        for alt in alts {
            if alt.items.is_empty() {
                result.push(Alternative {
                    symbols: Vec::new(),
                    weight: 1.0,
                    semantic_tag: None,
                });
                continue;
            }
            let sym_refs = self.build_items(&alt.items)?;
            result.push(Alternative {
                symbols: sym_refs,
                weight: 1.0,
                semantic_tag: None,
            });
        }
        if result.is_empty() {
            return Err(ParseError {
                line: rule_line,
                column: rule_col,
                message: "empty rule body".into(),
            });
        }
        Ok(result)
    }

    pub fn build_items<I: BuildItem>(&mut self, items: &[I]) -> Result<Vec<SymbolRef>, ParseError> {
        let mut refs = Vec::with_capacity(items.len());
        for item in items {
            I::build_item(self, item, &mut refs)?;
        }
        Ok(refs)
    }

    // ── Convenience methods for common item types ────────────────────────

    pub fn build_literal(&mut self, s: &str, refs: &mut Vec<SymbolRef>) {
        let sid = self.alloc_symbol(Symbol::Terminal(TerminalKind::Literal(
            s.as_bytes().to_vec(),
        )));
        refs.push(SymbolRef {
            symbol: sid,
            modifier: Modifier::Once,
        });
    }

    pub fn build_nonterminal(
        &mut self,
        name: &str,
        line: usize,
        col: usize,
        refs: &mut Vec<SymbolRef>,
    ) -> Result<(), ParseError> {
        let pid = self.name_to_id.get(name).ok_or_else(|| ParseError {
            line,
            column: col,
            message: format!("undefined rule: {:?}", name),
        })?;
        let sid = self.alloc_symbol(Symbol::NonTerminal(*pid));
        refs.push(SymbolRef {
            symbol: sid,
            modifier: Modifier::Once,
        });
        Ok(())
    }

    pub fn build_charclass(
        &mut self,
        ranges: &[(u8, u8)],
        negated: bool,
        refs: &mut Vec<SymbolRef>,
    ) {
        let sid = self.alloc_symbol(Symbol::Terminal(TerminalKind::CharClass {
            ranges: ranges.to_vec(),
            negated,
        }));
        refs.push(SymbolRef {
            symbol: sid,
            modifier: Modifier::Once,
        });
    }

    pub fn build_anychar(&mut self, refs: &mut Vec<SymbolRef>) {
        let sid = self.alloc_symbol(Symbol::Terminal(TerminalKind::AnyByte));
        refs.push(SymbolRef {
            symbol: sid,
            modifier: Modifier::Once,
        });
    }

    pub fn build_group<I: BuildItem>(
        &mut self,
        alts: &[RawAlternative<I>],
        refs: &mut Vec<SymbolRef>,
    ) -> Result<(), ParseError> {
        if alts.len() == 1 && !alts[0].items.is_empty() {
            let inner_refs = self.build_items(&alts[0].items)?;
            if inner_refs.len() == 1 {
                refs.push(inner_refs.into_iter().next().unwrap());
            } else {
                let sid = self.push_anon_production(vec![Alternative {
                    symbols: inner_refs,
                    weight: 1.0,
                    semantic_tag: None,
                }]);
                refs.push(SymbolRef {
                    symbol: sid,
                    modifier: Modifier::Once,
                });
            }
        } else {
            let built_alts = self.build_alternatives(alts, 0, 0)?;
            let sid = self.push_anon_production(built_alts);
            refs.push(SymbolRef {
                symbol: sid,
                modifier: Modifier::Once,
            });
        }
        Ok(())
    }

    pub fn build_quantified<I: BuildItem>(
        &mut self,
        inner: &I,
        quantifier: RawQuantifier,
        refs: &mut Vec<SymbolRef>,
    ) -> Result<(), ParseError> {
        let modifier = match quantifier {
            RawQuantifier::Optional => Modifier::Optional,
            RawQuantifier::ZeroOrMore => Modifier::ZeroOrMore {
                min: 0,
                max: u32::MAX,
            },
            RawQuantifier::OneOrMore => Modifier::OneOrMore {
                min: 1,
                max: u32::MAX,
            },
        };
        let inner_sid = self.build_single_symbol(inner)?;
        refs.push(SymbolRef {
            symbol: inner_sid,
            modifier,
        });
        Ok(())
    }

    /// Build an item into a single SymbolId. If the item produces multiple
    /// SymbolRefs, wrap them in an anonymous production.
    pub fn build_single_symbol<I: BuildItem>(&mut self, item: &I) -> Result<SymbolId, ParseError> {
        let mut refs = Vec::with_capacity(1);
        I::build_item(self, item, &mut refs)?;

        if refs.len() == 1 && matches!(refs[0].modifier, Modifier::Once) {
            return Ok(refs[0].symbol);
        }

        let sid = self.push_anon_production(vec![Alternative {
            symbols: refs,
            weight: 1.0,
            semantic_tag: None,
        }]);
        Ok(sid)
    }

    /// Build a single SymbolId from a list of alternatives.
    /// If there's a single alternative with a single item, returns that symbol directly.
    /// Otherwise wraps in an anonymous production.
    pub fn build_alts_symbol<I: BuildItem>(
        &mut self,
        alts: &[RawAlternative<I>],
    ) -> Result<SymbolId, ParseError> {
        if alts.len() == 1 && alts[0].items.len() == 1 {
            let mut refs = Vec::with_capacity(1);
            I::build_item(self, &alts[0].items[0], &mut refs)?;
            if refs.len() == 1 && matches!(refs[0].modifier, Modifier::Once) {
                return Ok(refs[0].symbol);
            }
        }

        let built_alts = self.build_alternatives(alts, 0, 0)?;
        let sid = self.push_anon_production(built_alts);
        Ok(sid)
    }
}

/// Build a complete `GrammarIr` from a parsed raw grammar.
///
/// `pick_start` determines the start production (ANTLR picks first lowercase rule,
/// EBNF/PEG use index 0).
pub fn build_ir<I: BuildItem>(
    raw: RawGrammar<I>,
    pick_start: fn(&[RawRule<I>]) -> ProductionId,
) -> Result<GrammarIr, ParseError> {
    let n_named = raw.rules.len();

    let mut name_to_id: HashMap<String, ProductionId> = HashMap::with_capacity(n_named);
    for (i, rule) in raw.rules.iter().enumerate() {
        match name_to_id.entry(rule.name.clone()) {
            std::collections::hash_map::Entry::Occupied(_) => {
                return Err(ParseError {
                    line: rule.line,
                    column: rule.col,
                    message: format!("duplicate rule: {:?}", rule.name),
                });
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(ProductionId(i as u32));
            }
        }
    }

    let mut symbols: Vec<Symbol> = Vec::new();
    let mut anon_productions: Vec<Production> = Vec::new();

    let mut named_productions: Vec<Production> = Vec::with_capacity(n_named);
    for (i, rule) in raw.rules.iter().enumerate() {
        let mut builder = IrBuilder {
            symbols: &mut symbols,
            name_to_id: &name_to_id,
            anon_productions: &mut anon_productions,
            n_named: n_named as u32,
        };
        let alts = builder.build_alternatives(&rule.alternatives, rule.line, rule.col)?;
        named_productions.push(Production {
            id: ProductionId(i as u32),
            name: rule.name.clone(),
            alternatives: alts,
            attrs: ProductionAttrs::default(),
        });
    }

    let mut productions = named_productions;
    productions.extend(anon_productions);

    let start = pick_start(&raw.rules);

    let mut ir = GrammarIr {
        productions,
        symbols,
        start,
        token_pools: Vec::new(),
    };

    compute_min_depths(&mut ir);
    mark_recursive(&mut ir);
    ir.validate().map_err(|e| ParseError {
        line: 0,
        column: 0,
        message: format!("IR validation failed: {e}"),
    })?;

    Ok(ir)
}
