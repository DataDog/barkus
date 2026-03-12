# ADR-0007: Configuration and Profiles

## Status

Proposed

## Context and Problem Statement

In fuzz-generators, config is a single JSON blob per generator (e.g., `SqlConfig` bundles dialect, table names, and `MutatorConfig`). No way to share a compiled grammar across campaigns with different mutation policies without re-parsing. How should barkus separate grammar compilation from generation policy?

## Decision Drivers

- Grammar compilation is expensive (parsing, lowering, precomputing min_depth/recursion).
- Different fuzzing campaigns need different policies on the same grammar.
- Profiles must be safe for concurrent use in multi-threaded fuzzing.

## Considered Options

1. **Separate Grammar and Profile handles**
2. Single config blob (status quo from fuzz-generators)
3. Mutable profiles adjustable during fuzzing
4. Inline configuration in grammar annotations

## Decision Outcome

Chosen option: **Option 1 — Separate Grammar and Profile handles.**

**Grammar handle** — compiled, immutable, `Send + Sync`:
- `GrammarIr` or `SchemaIr` (production tables, indexes, min_depth, recursion flags).
- No mutation parameters, no budgets, no dictionaries.
- Created via `Grammar::compile(ir)` or frontend: `barkus_antlr::compile(source, opts)`.

**Profile handle** — mutation/generation policy, immutable, `Send + Sync`:
- `validity_mode`: Strict / NearValid / Havoc
- `max_depth`, `max_total_nodes`: size budgets
- `repetition_bounds`: default (min, max)
- `dictionary`: token dictionary for injection
- `havoc_intensity`: 0.0–1.0
- `rule_overrides`: per-rule weight, depth, repetition, dictionary (from sidecar)
- `semantic_hooks`: pluggable post-generation repair

**Sidecar format** (JSON — portable across languages, passable as a string through the FFI layer):
```json
{
  "profile": {
    "validity_mode": "strict",
    "max_depth": 12,
    "max_total_nodes": 5000,
    "havoc_intensity": 0.3
  },
  "rules": {
    "selectStatement": { "weight": 2.0, "max_depth": 5 },
    "identifier": { "dictionary": ["users", "orders", "id", "name"] }
  }
}
```

**Usage:**
```rust
let grammar = barkus_antlr::compile(&source, &opts)?;
let strict = Profile::builder().validity_mode(Strict).build();
let havoc = Profile::builder().validity_mode(Havoc).havoc_intensity(0.8).build();
```

**FFI:** Two separate opaque handles — `barkus_grammar_compile()` and `barkus_profile_create()`.

### Pros

- Grammar parsed once, reused with different profiles.
- Immutable + `Send + Sync` → safe concurrent use.
- Sidecar config versioned independently.

### Cons

- Slightly more complex FFI (two create/destroy pairs).
- Profile immutability means creating a new handle to change any parameter.
