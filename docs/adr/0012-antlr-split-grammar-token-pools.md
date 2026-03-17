# ADR-0012: ANTLR Split Grammar and Token Pool Strategy

## Status

Accepted

## Context and Problem Statement

Production SQL grammars in the [grammars-v4](https://github.com/antlr/grammars-v4) repository use ANTLR's split-grammar convention: a `*Lexer.g4` file defining token rules and a `*Parser.g4` file defining parser rules that reference tokens by name. The existing `barkus-antlr` compiler handles combined grammars (lexer + parser rules in one file) but cannot process split grammars. How should barkus compile split ANTLR grammars into its normalized IR, and how should lexer rules be represented for generation?

## Decision Drivers

- SQL grammars from grammars-v4 (PostgreSQL, Trino, SQLite) all use the split convention.
- Lexer rules define the universe of valid tokens — an `IDENTIFIER` rule specifies what character sequences are legal identifiers. For fuzzing, we want to either mechanically generate from the lexer rule or override it with semantically meaningful values (real table/column names).
- The semantic hooks mechanism ([ADR-0011](0011-semantic-hook-architecture.md)) needs a way to intercept token generation at the lexer-rule level.

## Considered Options

1. **Vendor .g4 files; compile split grammars by mapping lexer rules to `TokenPool` entries; hooks expand pools at generation time**
2. Merge lexer and parser .g4 files into a combined grammar as a pre-processing step
3. Generate a separate lexer automaton and feed tokens into the parser IR
4. Manually transcribe SQL grammars into EBNF

## Decision Outcome

Chosen option: **Option 1 — TokenPool + hooks unifies lexer expansion and semantic override.**

### Grammar vendoring

ANTLR SQL grammars are vendored into `grammars/antlr-sql/` at the repository root:

```
grammars/antlr-sql/
├── postgresql/
│   ├── PostgreSQLLexer.g4
│   ├── PostgreSQLParser.g4
│   └── LICENSE
├── sqlite/
│   ├── SQLiteLexer.g4
│   ├── SQLiteParser.g4
│   └── LICENSE
├── trino/
│   ├── TrinoLexer.g4
│   ├── TrinoParser.g4
│   └── LICENSE
├── SOURCE.md       # grammars-v4 repo link + pinned commit SHA
└── update.sh       # fetch script for updating from upstream
```

Each grammar directory includes its original LICENSE file. `SOURCE.md` records the upstream repository URL and the pinned commit SHA. `update.sh` downloads files from `raw.githubusercontent.com` at the pinned commit.

### `compile_split()` function

A new public function in `barkus-antlr`:

```rust
pub fn compile_split(
    lexer_source: &str,
    parser_source: &str,
) -> Result<GrammarIr, ParseError>
```

The compilation pipeline:
1. Parse the lexer source into lexer rules (name → body).
2. Parse the parser source into parser rules (the usual raw grammar).
3. During IR building, when a parser rule references a name that matches a lexer rule:
   - Create a `TokenPoolEntry` from the lexer rule's body (alternatives of terminal sequences).
   - Emit `TerminalKind::TokenPool(pool_id)` in the parser rule's IR instead of a `NonTerminal` reference.
4. `fragment` rules in the lexer are inlined into their referencing lexer rules — they are not exposed as pools.
5. Lexer rules with `-> skip` or `-> channel(HIDDEN)` are identified as whitespace/comments and excluded from pool creation.
6. Lexer modes (`pushMode`, `popMode`) are logged as warnings and flattened for now.

### TokenPool in the IR

`TokenPoolEntry` in `GrammarIr` represents a lexer rule's expansion:

```rust
pub struct TokenPoolEntry {
    pub name: String,
    pub alternatives: Vec<Alternative>,
}
```

Each pool entry is a set of alternatives (mirroring the lexer rule's `|`-separated bodies). During generation, `TerminalKind::TokenPool(pool_id)` consumes a tape byte to select an alternative, then expands the alternative's symbols to produce terminal bytes.

### Hook integration

The `on_token_pool` hook ([ADR-0011](0011-semantic-hook-architecture.md)) intercepts pool expansion. For SQL generation, `SqlHooks` maps pool IDs to lexer rule names (e.g., `IDENTIFIER`, `NUMBER_LITERAL`) and overrides expansion with semantically valid values from the current scope.

### Implicit whitespace

Split ANTLR grammars assume the lexer handles whitespace between tokens. `compile_split` inserts implicit whitespace (a single space literal) between consecutive parser tokens. This is configurable and can be overridden by hooks.

### Pros

- Directly consumes upstream grammars with no manual transcription.
- TokenPool + hooks provides a clean seam for semantic override at the token level.
- Vendoring with pinned commit ensures reproducibility.
- `fragment` inlining and skip-token exclusion handle the most common ANTLR lexer features.

### Cons

- ANTLR lexer features not yet supported: modes, predicates, embedded actions. These are rare in SQL grammars but would need handling for other languages.
- Implicit whitespace insertion is a heuristic — some grammars may need explicit whitespace rules.
- Vendoring adds files to the repository that must be kept in sync with upstream.

## Links

- [grammars-v4 repository](https://github.com/antlr/grammars-v4)
- [ADR-0003: Grammar Frontend Architecture](0003-grammar-frontend-architecture.md)
- [ADR-0008: SQL Semantic Generation Layer](0008-sql-semantic-generation-layer.md)
- [ADR-0011: Semantic Hook Architecture](0011-semantic-hook-architecture.md)
