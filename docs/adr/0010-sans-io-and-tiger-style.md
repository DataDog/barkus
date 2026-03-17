# ADR-0010: Sans I/O and Tiger Style Code Approach

## Status

Accepted

## Context and Problem Statement

Barkus is a library used in security-critical fuzzing contexts — bugs in the generator can mask bugs in the target. The library must be robust, deterministic, and testable. How should barkus structure its code to maximize correctness and prevent subtle failures?

## Decision Drivers

- TigerBeetle's "Tiger Style" guidelines: heavy assertions, deterministic behavior, explicit resource limits, no hidden allocation in hot paths, defense-in-depth.
- The Sans I/O pattern (originated in the Python community, e.g., `sans-io.readthedocs.io`): protocol/logic core does zero I/O; all I/O is pushed to the caller. This makes the core trivially testable, embeddable, and deterministic.
- Fuzz-generators has no assertions beyond Rust's type system — silent fallback on malformed state rather than fail-fast.
- The decision tape codec ([ADR-0002](0002-decision-tape-and-havoc-paradox.md)) and mutation engine ([ADR-0004](0004-structure-aware-mutation-strategy.md)) have complex invariants that benefit from runtime checking.

## Considered Options

1. **Sans I/O core + Tiger Style assertions and resource discipline**
2. Standard Rust idioms (Result/Option, no extra assertions)
3. Formal verification of core invariants
4. Defensive programming with extensive error returns

## Decision Outcome

Chosen option: **Option 1 — Sans I/O + Tiger Style.**

### Sans I/O Principle

`barkus-core` performs **zero I/O**. No file reads, no network, no stdout, no logging framework. All data flows through function arguments and return values:

- Grammar source text is passed in as `&[u8]` or `&str` — the caller reads the file.
- Sidecar configuration is passed as a JSON `&str` — the caller reads and provides it.
- Generated output is written to caller-provided `&mut Vec<u8>` — the caller decides what to do with it.
- Error diagnostics are returned as structured types — the caller decides how to report them.
- Random bytes come from caller-provided `&mut impl Rng` or `&[u8]` tape — the caller owns the entropy source.

Only the adapter crate (`barkus-ffi`) and the Go bindings perform I/O. The FFI layer reads nothing from disk; it receives all data as byte slices from the caller.

This makes `barkus-core`:
- **Deterministic**: same inputs → same outputs, always.
- **Embeddable**: works in `no_std` contexts (with `alloc`), WASM, in-process fuzzers, and test harnesses.
- **Trivially testable**: no mocking, no test fixtures on disk, no environment setup.

### Tiger Style Assertions

Barkus uses `debug_assert!` liberally in development and `assert!` for invariants that must hold even in release:

**Always-on assertions (release + debug):**
- **Resource budgets**: generation aborts (not silently truncates) if `max_total_nodes` or `max_depth` is exceeded. The caller gets an explicit `BudgetExhausted` error.
- **Index bounds**: `ProductionId` and `SymbolId` are validated against the IR's length at decode time. Out-of-bounds → panic (indicates a bug in the compiler/frontend, not in the input).
- **Tape-AST consistency**: after decode, the `TapeMap` length matches the number of decision points actually consumed. Mismatch → panic.
- **Determinism check**: in test builds, `generate()` is called twice with the same RNG seed and the results are compared. Divergence → panic.

**Debug-only assertions (`debug_assert!`):**
- IR well-formedness: every `NonTerminal(id)` references a valid `ProductionId`, every alternative is non-empty, `min_depth` is consistent.
- Mutation invariants: after AST mutation in strict mode, the AST still validates against the grammar.
- Tape locality: after a single-byte tape mutation, the structural diff is bounded.

### Explicit Resource Limits (No Unbounded Growth)

- `Vec` pre-allocations use `with_capacity` based on profile budgets, not unbounded `push`.
- Recursion during generation uses an explicit depth counter, not the call stack. Stack overflow is impossible.
- Repetition (`ZeroOrMore`, `OneOrMore`) is bounded by `profile.repetition_bounds` — never unbounded.
- `FragmentDb` has a configurable max size; old entries are evicted.
- The serialization buffer has a `max_output_len` cap; exceeding it returns `BudgetExhausted`.

### No Hidden Allocation in Hot Paths

The generation/decode hot path:
- Reuses a pre-allocated `Vec<u8>` output buffer (caller-provided or internally cached).
- Reuses a pre-allocated `Vec<AstNode>` for the AST (pool per thread).
- Does not allocate `String` or `HashMap` during tape decode — all lookups are indexed by dense integer IDs.
- Mutation operates in-place on the AST where possible.

### Fail-Fast, Not Fail-Silent

Unlike fuzz-generators which silently returns `GenError::InsufficientInput` for many error conditions, barkus distinguishes:
- **Expected conditions** (short tape, budget exhausted) → `Result::Err` with a typed error.
- **Invariant violations** (IR inconsistency, index out of bounds, determinism failure) → `panic!` / `assert!`. These indicate bugs in barkus itself, not in the input.
- **Never**: silent fallback to a default value when the real answer is "this is a bug."

### Pros

- Deterministic, embeddable, trivially testable core.
- Assertions catch bugs at the point of introduction, not downstream.
- Explicit budgets prevent OOM and stack overflow in production fuzzing campaigns.
- No hidden allocation means predictable performance.

### Cons

- More assertions = slightly more code to maintain.
- Always-on assertions have a small runtime cost (index checks, budget checks).
- `no_std` compatibility constrains which Rust features can be used in core (no `std::fs`, `std::io`, etc.).

## Links

- TigerBeetle Style Guide: https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md
- Sans I/O: https://sans-io.readthedocs.io/
- Rust `no_std` guide: https://docs.rust-embedded.org/book/intro/no-std.html
