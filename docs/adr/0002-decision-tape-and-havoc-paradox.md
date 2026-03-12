# ADR-0002: Decision Tape and the Havoc Paradox

## Status

Proposed

## Context and Problem Statement

In fuzz-generators, generators consume bytes from `Unstructured` (the `arbitrary` crate) to make structural decisions. `Unstructured` consumes **variable** numbers of bytes per decision. This creates the **havoc paradox**: when the fuzzer mutates a byte, the boundary shift cascades — a single flipped byte changes where every subsequent read starts, scrambling the entire output tree rather than perturbing one node.

How should barkus map input bytes to structural decisions while preserving locality under mutation?

## Decision Drivers

- Padhye et al., "Semantic Fuzzing with Zest" (ISSTA 2019) showed parametric fuzzing works when byte-to-structure mapping preserves locality.
- Liyanage et al., "Zeugma: Parametric Fuzzing with Structure-Aware Crossover" (ISSTA 2023) identified that naive crossover on decision streams destroys structure.
- Jiang et al., "The Havoc Paradox in Generator-Based Fuzzing" (ICSE 2025) formally characterized the locality problem.
- Go native fuzzing (`testing.F`) provides `[]byte` — the tape must be raw bytes.

## Considered Options

1. **Fixed-width decision tape** with control header
2. Keep using `arbitrary::Unstructured` (status quo)
3. Variable-length encoding with alignment markers
4. Typed `Vec<Decision>` instead of raw bytes

## Decision Outcome

Chosen option: **Option 1 — Fixed-width decision tape.**

Each decision is encoded at a **fixed, pre-known width** per production/symbol:

- `Choice` with N alternatives: `tape[offset] % N`
- `ZeroOrMore(min, max)`: `tape[offset] % (max - min + 1) + min`
- `CharClass` with K chars: `tape[offset] % K`
- `Optional`: `tape[offset] & 1`

Byte position N always corresponds to the same decision point → **perfect locality**.

**Two generation modes:**
- `generate(rng, grammar, profile) -> (Ast, DecisionTape)` — fill tape from RNG, then decode.
- `decode(tape, grammar, profile) -> Ast` — deterministic decode from existing tape.

**Total decoder:** Any tape produces output. Short tape → fallback to shallowest alternative (`min_depth`-biased). Long tape → extra bytes ignored. No rejection.

**Control header** (first 2 bytes):
- Byte 0: validity mode (0x00=strict, 0x01=near-valid, 0x02=havoc)
- Byte 1: reserved

**Tape metadata:** `TapeMap` maps tape byte ranges → AST node IDs, enabling targeted mutation.

### Pros

- Solves the havoc paradox: one flipped byte perturbs one decision.
- Compact, deterministic, replay-friendly.
- Go native fuzzing works directly ([]byte = tape).

### Cons

- Wastes some entropy (3 alternatives use a full byte where 2 bits suffice).
- Fixed-width assumes static grammar structure — dynamic/context-dependent grammars need workarounds.

## Links

- Padhye et al., "Semantic Fuzzing with Zest," ISSTA 2019
- Liyanage et al., "Zeugma: Parametric Fuzzing with Structure-Aware Crossover," ISSTA 2023
- Jiang et al., "The Havoc Paradox in Generator-Based Fuzzing," ICSE 2025
