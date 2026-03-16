# ADR-0001: Normalized Intermediate Representation (IR)

## Status

Proposed

## Context and Problem Statement

In fuzz-generators, each grammar format has its own AST types and generation logic. ANTLR defines `AntlrExpr` (Literal, CharClass, Choice, Sequence, ZeroOrMore, etc.), PEG defines its own `Expr` with nearly identical variants, and SQL has hand-written AST nodes. Every cross-cutting feature (mutation, decision tape, shrinking) must be reimplemented per frontend.

How should barkus represent grammars internally so that generation, mutation, and shrinking are implemented once?

## Decision Drivers

- Nautilus (NDSS 2019), Gramatron (ISSTA 2021), and Grimoire (USENIX Security 2019) all converge on a single canonical grammar IR for mutation and generation.
- Gramatron (Srivastava et al., ISSTA 2021) showed that uniform random CFG expansion produces structural bias; depth-aware alternative selection requires precomputed `min_depth` per production.
## Considered Options

1. **Single normalized IR in barkus-core**
2. Keep per-format ASTs (status quo from fuzz-generators)
3. Use tree-sitter grammars as the IR

## Decision Outcome

Chosen option: **Option 1 — Single normalized IR.**

Barkus defines a single normalized IR in `barkus-core`:

- **GrammarIr** — top-level compiled grammar. `Vec<Production>` indexed by dense `ProductionId(u32)`, `Vec<Symbol>` indexed by `SymbolId(u32)`, and `start: ProductionId`.
- **Production** — named nonterminal: `{ id, name, alternatives: Vec<Alternative>, attributes: ProductionAttrs }`.
- **Alternative** — sequence of symbol refs: `{ symbols: Vec<SymbolRef>, weight: f32, semantic_tag: Option<String> }`.
- **SymbolRef** — symbol + modifier: `{ symbol: SymbolId, modifier: Modifier }` where `Modifier` is `Once | Optional | ZeroOrMore(min, max) | OneOrMore(min, max)`.
- **Symbol** — `Terminal(TerminalKind) | NonTerminal(ProductionId)`.
- **TerminalKind** — `Literal(Vec<u8>) | CharClass { ranges, negated } | AnyByte | ByteRange(u8, u8) | TokenPool(PoolId)`.
- **ProductionAttrs** — `{ min_depth, is_recursive, token_kind, semantic_hook }`.

Dense integer IDs enable O(1) lookup. `min_depth` per production enables depth-aware alternative selection.

### Pros

- All mutation operators implemented once against the IR.
- New grammar formats need only a frontend parser.
- Precomputed `min_depth` and `is_recursive` enable smart generation budgeting.

### Cons

- PEG ordered choice, ANTLR predicates, and other format-specific semantics are lowered as best-effort approximations — some fidelity loss.

## Links

- Aschermann et al., "Nautilus: Fishing for Deep Bugs with Grammars," NDSS 2019
- Srivastava et al., "Gramatron: Effective Grammar-Aware Fuzzing," ISSTA 2021
- Blazytko et al., "GRIMOIRE: Synthesizing Structure while Fuzzing," USENIX Security 2019
