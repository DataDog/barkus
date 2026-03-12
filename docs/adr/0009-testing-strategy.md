# ADR-0009: Testing Strategy

## Status

Proposed

## Context and Problem Statement

Barkus is a fuzzing library — it must be both correct (generators produce valid output) and robust (mutation preserves invariants, the tape codec is deterministic). How should barkus itself be tested?

## Decision Drivers

- Fuzz-generators has unit tests per generator verifying determinism and basic output validity, but no property-based testing, no roundtrip testing, and no self-fuzzing.
- The decision tape codec ([ADR-0002](0002-decision-tape-and-havoc-paradox.md)) has strong invariants (determinism, totality, locality) that are natural property-test targets.
- Grammar frontends compile external formats — conformance against reference implementations matters.
- The library will be used in security-critical fuzzing contexts; bugs in the generator can mask bugs in the target.

## Considered Options

1. **Multi-layer testing: unit + property-based + self-fuzzing + conformance**
2. Unit tests only (status quo from fuzz-generators)
3. Unit + integration tests only
4. Formal verification of core invariants

## Decision Outcome

Chosen option: **Option 1 — Multi-layer testing.**

**Layer 1 — Unit tests (`cargo test`):**
- Each IR type: construction, serialization, indexing.
- Each frontend: parse known grammars, verify `GrammarIr` structure.
- SQL semantic layer: scope resolution, type checking, dialect serialization.
- FFI boundary: null checks, error handling, memory ownership.

**Layer 2 — Property-based tests (`proptest` / `quickcheck`):**
- **Tape determinism**: `∀ tape, grammar, profile: decode(tape, g, p) == decode(tape, g, p)`.
- **Tape totality**: `∀ tape: decode(tape, g, p)` succeeds (never panics, never returns error).
- **Tape locality**: `∀ tape, i: hamming_distance(decode(tape, g, p), decode(flip(tape, i), g, p))` is small (measures structural diff, not byte diff). See `crates/barkus-core/tests/locality_tests.rs`.
- **Generate-decode roundtrip**: `∀ rng: let (ast, tape) = generate(rng, g, p); decode(tape, g, p) == ast`.
- **Mutation validity**: `∀ ast in strict mode: mutate(ast, ...) produces valid output` (validated by re-parsing with a reference parser). See `crates/barkus-core/tests/mutation_tests.rs`.
- **Shrink monotonicity**: `∀ ast: shrink(ast)` produces strictly smaller ASTs that still reproduce the property.

**Layer 3 — Self-fuzzing (`cargo fuzz` / libFuzzer):**
- Fuzz the tape decoder: `fuzz_target!(|tape: &[u8]| { decode(tape, grammar, profile); })` — should never panic.
- Fuzz the frontends: `fuzz_target!(|grammar_src: &[u8]| { compile(grammar_src); })` — should never panic on malformed input.
- Fuzz the mutation engine: `fuzz_target!(|tape: &[u8]| { let ast = decode(tape, ...); mutate(ast, ...); })`.
- Fuzz the FFI boundary: random call sequences to the C API.

**Layer 4 — Conformance tests:**
- ANTLR frontend: parse grammars from `antlr/grammars-v4`, generate outputs, validate with a reference ANTLR parser (Java) or tree-sitter parser.
- Protobuf: generate wire-format messages, decode with `prost` or `protobuf` crate, verify roundtrip.
- SQL: generate queries, parse with `sqlparser-rs`, verify AST structure matches intent.
- PEG: generate strings from DataDog PEG grammars (from fuzz-generators test fixtures), validate against the Go PEG parser.

**Layer 5 — Performance benchmarks (`criterion`):**
- Tape decode throughput (decisions/sec).
- Generation throughput (payloads/sec) per grammar size.
- Mutation throughput (mutations/sec).
- FFI call overhead.
- Memory usage per corpus entry (including `MutationMeta`).

### Pros

- Property tests catch invariant violations that unit tests miss (tape locality, roundtrip).
- Self-fuzzing is meta-appropriate: a fuzzing library should eat its own dogfood.
- Conformance tests catch frontend parsing drift from reference implementations.
- Benchmarks prevent performance regressions.

### Cons

- Conformance tests require external reference tools (Java ANTLR, Go PEG parser).
- Property tests and fuzzing are slow — should run in CI on a schedule, not on every push.
- Maintaining conformance test fixtures as upstream grammars evolve.

## Links

- Rust Fuzz Book: https://rust-fuzz.github.io/book/
- `proptest` crate documentation
- `criterion` crate documentation
