# ADR-0001: Normalized Intermediate Representation (IR)

## Status

Proposed

## Context and Problem Statement

In fuzz-generators, each grammar format has its own AST types and generation logic. ANTLR defines `AntlrExpr` (Literal, CharClass, Choice, Sequence, ZeroOrMore, etc.), PEG defines its own `Expr` with nearly identical variants, SQL has hand-written AST nodes, and protobuf uses yet another AST. Every cross-cutting feature (mutation, decision tape, shrinking) must be reimplemented per frontend.

How should barkus represent grammars internally so that generation, mutation, and shrinking are implemented once?

## Decision Drivers

- Nautilus (NDSS 2019), Gramatron (ISSTA 2021), and Grimoire (USENIX Security 2019) all converge on a single canonical grammar IR for mutation and generation.
- Gramatron (Srivastava et al., ISSTA 2021) showed that uniform random CFG expansion produces structural bias; depth-aware alternative selection requires precomputed `min_depth` per production.
- Protobuf's wire format (varint tags, length-delimited fields, zigzag encoding) is fundamentally different from text grammar productions.

## Considered Options

1. **Single normalized IR in barkus-core** (with separate `SchemaIr` for protobuf)
2. Keep per-format ASTs (status quo from fuzz-generators)
3. Single AST covering all formats including protobuf
4. Use tree-sitter grammars as the IR

## Decision Outcome

Chosen option: **Option 1 — Single normalized IR with separate SchemaIr for protobuf.**

Barkus defines a single normalized IR in `barkus-core`:

- **GrammarIr** — top-level compiled grammar. `Vec<Production>` indexed by dense `ProductionId(u32)`, `Vec<Symbol>` indexed by `SymbolId(u32)`, and `start: ProductionId`.
- **Production** — named nonterminal: `{ id, name, alternatives: Vec<Alternative>, attributes: ProductionAttrs }`.
- **Alternative** — sequence of symbol refs: `{ symbols: Vec<SymbolRef>, weight: f32, semantic_tag: Option<String> }`.
- **SymbolRef** — symbol + modifier: `{ symbol: SymbolId, modifier: Modifier }` where `Modifier` is `Once | Optional | ZeroOrMore(min, max) | OneOrMore(min, max)`.
- **Symbol** — `Terminal(TerminalKind) | NonTerminal(ProductionId)`.
- **TerminalKind** — `Literal(Vec<u8>) | CharClass { ranges, negated } | AnyByte | ByteRange(u8, u8) | TokenPool(PoolId)`.
- **ProductionAttrs** — `{ min_depth, is_recursive, token_kind, semantic_hook }`.

**Protobuf special path:** Protobuf compiles into `SchemaIr` (not `GrammarIr`) — a tree of message/field/enum descriptors with field numbers, wire types, and labels. `SchemaIr` shares `Modifier` and decision-tape codec with `GrammarIr` but has its own node types.

Dense integer IDs enable O(1) lookup. `min_depth` per production enables depth-aware alternative selection.

### Pros

- All mutation operators implemented once against the IR.
- New grammar formats need only a frontend parser.
- Precomputed `min_depth` and `is_recursive` enable smart generation budgeting.

### Cons

- PEG ordered choice, ANTLR predicates, and other format-specific semantics are lowered as best-effort approximations — some fidelity loss.
- Two IR types (`GrammarIr` + `SchemaIr`) rather than one.

## Links

- Aschermann et al., "Nautilus: Fishing for Deep Bugs with Grammars," NDSS 2019
- Srivastava et al., "Gramatron: Effective Grammar-Aware Fuzzing," ISSTA 2021
- Blazytko et al., "GRIMOIRE: Synthesizing Structure while Fuzzing," USENIX Security 2019
