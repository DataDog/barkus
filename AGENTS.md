# AGENTS.md — AI Code Assistant Guide

Barkus is a Rust workspace (with Go CGo bindings) that provides structure-aware fuzz generators. It is a from-scratch rewrite of fuzz-generators with a normalized IR, decision tape, and structure-aware mutation.

## Architecture Decision Records

Read these before making non-trivial changes. Each ADR explains a core design choice.

| ADR | Summary |
|-----|---------|
| [ADR-0001](docs/adr/0001-normalized-intermediate-representation.md) | Normalized IR that all grammar frontends lower into, so mutation/generation logic is written once |
| [ADR-0002](docs/adr/0002-decision-tape-and-havoc-paradox.md) | Fixed-width decision tape (one byte = one decision) to solve the havoc paradox |
| [ADR-0003](docs/adr/0003-grammar-frontend-architecture.md) | Grammar frontends only parse and lower to IR; generation is handled by the core engine |
| [ADR-0004](docs/adr/0004-structure-aware-mutation-strategy.md) | Structure-aware mutation that preserves syntactic validity while maximizing coverage |
| [ADR-0005](docs/adr/0005-fuzzer-integration-architecture.md) | FFI integration boundaries for Go native fuzzing and custom harnesses |
| [ADR-0006](docs/adr/0006-crate-structure.md) | Workspace crate organization with separated concerns (parser vs generator) |
| [ADR-0007](docs/adr/0007-configuration-and-profiles.md) | Configuration profiles that separate grammar compilation from generation policy |
| [ADR-0008](docs/adr/0008-sql-semantic-generation-layer.md) | SQL semantic layer for generating queries that pass beyond the parser |
| [ADR-0009](docs/adr/0009-testing-strategy.md) | Testing strategy: deterministic tape round-trips, snapshot tests, property tests |
| [ADR-0010](docs/adr/0010-sans-io-and-tiger-style.md) | Sans I/O + Tiger Style: no I/O in core, heavy assertions, fail-fast |
| [ADR-0011](docs/adr/0011-semantic-hook-architecture.md) | Semantic hooks for domain-specific logic (e.g. SQL scope/type checking) without coupling to core |
| [ADR-0012](docs/adr/0012-antlr-split-grammar-token-pools.md) | ANTLR split grammar and token pool strategy for lexer/parser separation |

## Key Best Practices

- **Sans I/O + Tiger Style** (ADR-0010): no I/O in core library code. Use heavy assertions and fail-fast. All side effects live at the boundary.
- **Preserve performance-sensitive code**: pre-allocations, `Vec::with_capacity`, buffer sizes, and capacity hints must survive refactors. Do not silently remove them.
- **Fixed-width decision tape** (ADR-0002): one byte = one decision. Never introduce variable-width encoding in the tape.
- **Run tests before submitting**: `cargo test --workspace` and `make test-go`.
- **Run linters**: `cargo fmt --all --check` and `cargo clippy --workspace`.
- **RFC process**: new features require an ADR. See [CONTRIBUTING.md](CONTRIBUTING.md) for the process.

## Build Commands

| Command | What it does |
|---------|-------------|
| `make ffi` | Build the Rust FFI library (`barkus-ffi`, release mode) |
| `make go-example` | Build the Go CLI example (`barkus-gen`) |
| `make test-go` | Run Go tests (builds FFI first) |
| `make test` | Run Go tests + `cargo test --workspace` |
| `make clean` | Remove all build artifacts |
