# ADR-0005: Fuzzer Integration Architecture

## Status

Proposed

## Context and Problem Statement

Fuzz-generators has a single FFI interface (4 C functions) with `bytes in → bytes out`. Barkus must support Go native fuzzing (`testing.F`) and custom harnesses. How should the integration boundaries be drawn?

## Decision Drivers

- Go's `testing.F.Fuzz` only provides `[]byte` — the tape must be raw bytes, deterministic, no hidden RNG.
- AFL++'s `LLVMCustomMutator` docs warn that crossover can violate structure.

## Considered Options

1. **Separate adapter crates per framework, tape-based FFI for Go**
2. Single FFI function (status quo)
3. Expose AST across FFI
4. Feature flags in core for fuzzer selection

## Decision Outcome

Chosen option: **Option 1 — Separate adapter crates.**

**Core API** (`barkus-core`) — pure Rust, no fuzzer deps:
```rust
Grammar::generate(&self, rng, profile) -> (Ast, DecisionTape)
Grammar::decode(&self, tape, profile) -> Ast
Grammar::mutate(&self, ast, tape, rng, profile) -> (Ast, DecisionTape)
Ast::serialize(&self, grammar, profile) -> Vec<u8>
```

**`barkus-ffi`** — C ABI, tape-aware, two-handle model:
- `barkus_grammar_compile(format_id, source, len, config, len) -> handle`
- `barkus_profile_create(config_json, len) -> handle`
- `barkus_render(grammar, profile, tape, len, output, len) -> status`
- `barkus_mutate_tape(grammar, profile, tape_in, len, tape_out, len) -> status`
- `barkus_{grammar,profile}_destroy(handle)`

Caller controls the tape. Go fuzzing passes corpus `[]byte` as tape. No hidden RNG.

### Pros

- Go: `[]byte` as tape via FFI, deterministic.
- No fuzzer framework dependencies in core.

### Cons

- More crates to maintain than a single FFI layer.
- FFI slightly more complex (two-handle model).

## Links

- Go testing documentation: `testing.F.Fuzz` function signature
- AFL++ documentation: `LLVMCustomMutator`, `AFL_CUSTOM_MUTATOR_ONLY`
