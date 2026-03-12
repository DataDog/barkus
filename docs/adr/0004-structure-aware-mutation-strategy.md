# ADR-0004: Structure-Aware Mutation Strategy

## Status

Proposed

## Context and Problem Statement

In fuzz-generators, mutation is byte-level only — 13 random byte ops applied to serialized output. These destroy syntactic structure. How should barkus mutate inputs to maximize coverage while controlling validity?

## Decision Drivers

- Aschermann et al., "Nautilus" (NDSS 2019): tree-based mutations found 2x more unique crashes than byte-level AFL.
- Wang et al., "Superion" (ICSE 2019): grammar-aware mutations on AST fragments.
- Jiang et al., "Gmutator" (ICSE 2023 demo): bugs live just outside the grammar boundary — near-valid inputs needed.
- Wang et al., "Skyfire" (IEEE S&P 2017): corpus-mined subtree splicing.
- LangFuzz (Holler et al., USENIX Security 2012): fragment recombination from existing corpora.

## Considered Options

1. **Three-level mutation (tape + AST + byte havoc) with validity modes**
2. Keep byte-level-only mutation (status quo)
3. AST-level mutation only (no tape-level or havoc)
4. Mutate serialized strings with parse-repair

## Decision Outcome

Chosen option: **Option 1 — Three-level mutation with validity modes.**

**Level 1 — Tape-level mutation (all modes):**
Structure-preserving due to fixed-width encoding ([ADR-0002](0002-decision-tape-and-havoc-paradox.md)):
- Point mutation: flip/arithmetic on a single tape byte → one decision changes.
- Range re-randomize: overwrite contiguous tape range → regenerate a subtree.
- Splice: replace tape region with corresponding region from another corpus entry at same production (via `TapeMap`).

**Level 2 — AST-level mutation (strict and near-valid):**
- Subtree regeneration from grammar.
- Compatible subtree splice from `FragmentDb` indexed by `ProductionId`.
- Toggle optional nodes.
- Perturb repetition count (±1 iteration).
- Shrink/expand literals.
- Swap siblings (commutative sequences).
- Dictionary injection at grammar-legal points.
- Semantic repair post-pass (strict mode only).

**Level 3 — Byte-level havoc (havoc mode only):**
After serialization, apply the 13 byte-level ops from fuzz-generators. For finding parser bugs outside the grammar boundary.

**Three validity modes (tape header byte 0):**
- **Strict** (0x00): L1 + L2 + semantic repair. Always valid.
- **NearValid** (0x01): L1 + L2 without repair + targeted violations.
- **Havoc** (0x02): L1 + L3. Grammar-unaware byte corruption.

**Corpus fragment reuse:** Parse existing inputs into ASTs, decompose per-production, store in `FragmentDb`.

**Per-input metadata:** `MutationMeta`: `TapeMap`, nonterminal positions by `ProductionId`, subtree sizes, depth.

### Pros

- Covers the full spectrum: valid → semantic bugs, near-valid → parser boundary bugs, havoc → robustness.
- Corpus fragment reuse leverages existing test suites.
- Tape-level mutation is cheap; AST-level is precise.

### Cons

- AST-level mutation more expensive than raw byte ops.
- `MutationMeta` adds memory overhead per corpus entry.
- `FragmentDb` requires initial corpus or warm-up phase.

## Links

- Aschermann et al., "Nautilus: Fishing for Deep Bugs with Grammars," NDSS 2019
- Wang et al., "Superion: Grammar-Aware Greybox Fuzzing," ICSE 2019
- Jiang et al., "Gmutator: A Mutation Approach for Fuzz Testing," ICSE 2023
- Wang et al., "Skyfire: Data-Driven Seed Generation for Fuzzing," IEEE S&P 2017
- Holler et al., "Fuzzing with Code Fragments," USENIX Security 2012
