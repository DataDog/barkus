# ADR-0003: Grammar Frontend Architecture

## Status

Accepted

## Context and Problem Statement

Fuzz-generators has four frontends that each contain both parsing and generation logic. With the normalized IR ([ADR-0001](0001-normalized-intermediate-representation.md)), frontends should only parse and lower. How should barkus structure grammar frontends and their configuration?

## Decision Drivers

- Grammarinator (Hodován et al., ICST 2018) showed that leveraging the large existing corpus of ANTLR v4 grammars (hundreds at `antlr/grammars-v4`) is a major practical advantage.
- Grammar files from external sources must be usable without modification.
- Per-rule generation weights and limits are needed but should not pollute grammar source.

## Considered Options

1. **Separate frontend crates with sidecar configuration**
2. Embed configuration in grammar annotations/comments
3. Single crate with feature flags for all formats
4. Runtime grammar interpretation (no compile step)

## Decision Outcome

Chosen option: **Option 1 — Separate frontend crates with sidecar config.**

Frontend crates each expose: `compile(source, options) -> Result<GrammarIr>`.

- **`barkus-antlr`**: ANTLR v4 combined/split grammars. Actions stripped but logged.
- **`barkus-ebnf`**: ISO 14977 EBNF and common variants.
- **`barkus-peg`**: PEG grammars (Bryan Ford notation and DataDog `peg` variant).

**Sidecar configuration** (JSON, external to grammar source — JSON chosen for cross-language portability, especially Go FFI):
```json
{
  "rules": {
    "select_stmt": { "weight": 2.0, "max_depth": 5 },
    "identifier": { "dictionary": ["users", "orders", "id"] }
  }
}
```

**Frontend-specific lowering:**
- PEG ordered choice (`/`) → IR `Choice` with descending weights (first alternative highest).
- ANTLR lexer modes → separate production groups with mode-switch semantic hooks.
- ANTLR predicates and PEG lookaheads → stripped.

### Pros

- Adding a grammar format = one new crate with `compile`.
- Third-party grammars (antlr/grammars-v4) usable unmodified.
- Sidecar config versioned independently from grammar.

### Cons

- Some grammar features (semantic predicates, context-dependent lexing) are best-effort approximations.
- Sidecar config adds an extra file to manage.

## Links

- Hodován et al., "Grammarinator: A Grammar-Based Open Source Fuzzer," ICST 2018
