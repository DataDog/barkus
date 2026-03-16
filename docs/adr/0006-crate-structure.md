# ADR-0006: Crate Structure

## Status

Proposed

## Context and Problem Statement

Fuzz-generators uses 6 crates with mixed concerns (each frontend contains both parser and generator). How should barkus organize its workspace?

## Decision Drivers

- Separation of concerns: IR, tape codec, mutation, frontends, domain generators, FFI, and fuzzer adapters are distinct responsibilities.
- Grammar parsing dependencies are heavy — not every user needs every format.
- `barkus-core` must be usable standalone.

## Considered Options

1. **Workspace of focused crates**
2. Monolithic crate with feature flags
3. Merge frontends into core
4. Separate mutation into its own crate

## Decision Outcome

Chosen option: **Option 1 — Workspace of focused crates.**

```
barkus/
  Cargo.toml                    (workspace)
  crates/
    barkus-core/                IR, AST, decision tape codec, mutation engine, shrinker, FragmentDb
    barkus-antlr/               ANTLR v4 parser -> GrammarIr
    barkus-ebnf/                EBNF parser -> GrammarIr
    barkus-peg/                 PEG parser -> GrammarIr
    barkus-sql/                 SQL domain generator (GrammarIr + semantic hooks)
    barkus-ffi/                 C ABI for Go and other languages
  go/                           Go bindings package
  docs/adr/                     Architecture decision records
  fuzz/                         Fuzz targets for self-testing
```

**Dependency graph:**
```
barkus-core              <- std, serde, rand (no fuzzer deps)
barkus-{antlr,ebnf,peg} <- barkus-core
barkus-sql               <- barkus-core
barkus-ffi               <- barkus-core + all frontends + libc
```

### Pros

- Adding a grammar format touches one crate.
- `barkus-core` usable standalone without any grammar parser.
- FFI links all frontends; Go gets everything. Rust users pick individual crates.

### Cons

- More workspace files to maintain than the original 6 crates.
- Cross-crate integration testing requires workspace-level test harnesses.
