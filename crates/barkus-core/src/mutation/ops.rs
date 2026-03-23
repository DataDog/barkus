//! Mutation operators and weighted dispatch (ADR-0004).
//!
//! Contains the individual Level 1 (tape-level) and Level 2 (structure-aware) mutation operators,
//! plus the top-level [`mutate`] function that selects an operator via weighted random choice.

use rand::{Rng, RngExt};

use crate::generate::generate_from;
use crate::ir::grammar::GrammarIr;
use crate::profile::Profile;
use crate::tape::map::ModifierByte;

use super::fragment_db::FragmentDb;
use super::MutationMeta;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationKind {
    PointMutate,
    RangeRerandomize,
    Splice,
    SubtreeRegenerate,
    ToggleOptional,
    PerturbRepetition,
}

// ── Helpers ──

/// Pick a random index from `0..len` where `predicate(index)` is true.
/// Uses reservoir sampling (single pass, no allocation).
/// Returns `None` if no element matches.
fn pick_random_matching(
    len: usize,
    rng: &mut impl Rng,
    predicate: impl Fn(usize) -> bool,
) -> Option<usize> {
    let mut chosen = None;
    let mut count = 0u32;
    for i in 0..len {
        if predicate(i) {
            count += 1;
            // Reservoir sampling: keep element i with probability 1/count
            if rng.random_range(0..count) == 0 {
                chosen = Some(i);
            }
        }
    }
    chosen
}

// ── Level 1: tape-level operators ──

/// Flip a random bit or apply ±1 arithmetic to a single body byte.
pub fn point_mutate(tape: &mut [u8], rng: &mut impl Rng) {
    if tape.len() <= 2 {
        return;
    }
    let idx = rng.random_range(2..tape.len());
    if rng.random_bool(0.5) {
        let bit = 1u8 << rng.random_range(0..8);
        tape[idx] ^= bit;
    } else if rng.random_bool(0.5) {
        tape[idx] = tape[idx].wrapping_add(1);
    } else {
        tape[idx] = tape[idx].wrapping_sub(1);
    }
}

/// Fill a random production's tape region with random bytes.
pub fn range_rerandomize(tape: &mut [u8], meta: &MutationMeta, rng: &mut impl Rng) {
    if meta.tape_map.entries.is_empty() {
        return;
    }
    let entry = &meta.tape_map.entries[rng.random_range(0..meta.tape_map.entries.len())];
    let start = entry.tape_offset;
    if start >= tape.len() {
        return;
    }
    let end = (start + entry.tape_len).min(tape.len());
    for byte in &mut tape[start..end] {
        *byte = rng.random();
    }
}

/// Replace a production's tape region with a fragment from the FragmentDb.
pub fn splice(
    tape: &mut Vec<u8>,
    meta: &MutationMeta,
    db: &FragmentDb,
    rng: &mut impl Rng,
) -> bool {
    let n = meta.tape_map.entries.len();
    if n == 0 {
        return false;
    }
    for _ in 0..10 {
        let entry = &meta.tape_map.entries[rng.random_range(0..n)];
        if let Some(fragment) = db.sample(entry.production_id, rng) {
            let start = entry.tape_offset;
            if start > tape.len() {
                continue;
            }
            let end = (start + entry.tape_len).min(tape.len());
            tape.splice(start..end, fragment.tape_bytes.iter().copied());
            return true;
        }
    }
    false
}

// ── Level 2: structure-aware operators ──

/// Pick a random non-root production and regenerate its subtree from scratch.
///
/// Uses [`generate_from`] to produce a fresh tape fragment for the selected production,
/// then splices the new fragment's body bytes into the tape, replacing the original region.
/// This effectively re-derives the subtree using fresh random decisions while preserving the
/// surrounding tape structure.
pub fn subtree_regenerate(
    tape: &mut Vec<u8>,
    meta: &MutationMeta,
    grammar: &GrammarIr,
    profile: &Profile,
    rng: &mut impl Rng,
) -> bool {
    let entries = &meta.tape_map.entries;
    let Some(idx) = pick_random_matching(entries.len(), rng, |i| {
        entries[i].tape_len > 0 && entries[i].tape_offset > 2
    }) else {
        return false;
    };

    let entry = &entries[idx];
    match generate_from(grammar, entry.production_id, profile, rng) {
        Ok((_ast, sub_tape, _map)) => {
            let sub_body = &sub_tape.bytes[2..];
            let start = entry.tape_offset;
            let end = (start + entry.tape_len).min(tape.len());
            tape.splice(start..end, sub_body.iter().copied());
            true
        }
        Err(_) => false,
    }
}

/// Toggle an optional modifier's choice byte (present ↔ absent).
///
/// Finds a random `ModifierByte::Optional` in the tape map and XORs its choice byte with 1,
/// flipping between the "present" and "absent" states. This is a minimal, targeted mutation
/// that exercises optional branch coverage.
pub fn toggle_optional(tape: &mut [u8], meta: &MutationMeta, rng: &mut impl Rng) -> bool {
    let mbs = &meta.tape_map.modifier_bytes;
    let Some(idx) = pick_random_matching(
        mbs.len(),
        rng,
        |i| matches!(mbs[i], ModifierByte::Optional { tape_offset } if tape_offset < tape.len()),
    ) else {
        return false;
    };

    let ModifierByte::Optional { tape_offset } = mbs[idx] else {
        unreachable!()
    };
    tape[tape_offset] ^= 1;
    true
}

/// Adjust a repetition count by ±1.
///
/// Finds a random `ModifierByte::Repetition` in the tape map and nudges its encoded count
/// up or down by one iteration (clamped to the `[min, max]` range). This explores
/// boundary conditions in repetition-dependent logic without drastically changing structure.
pub fn perturb_repetition(tape: &mut [u8], meta: &MutationMeta, rng: &mut impl Rng) -> bool {
    let mbs = &meta.tape_map.modifier_bytes;
    let Some(idx) = pick_random_matching(
        mbs.len(),
        rng,
        |i| matches!(mbs[i], ModifierByte::Repetition { tape_offset, .. } if tape_offset < tape.len()),
    ) else {
        return false;
    };

    let ModifierByte::Repetition {
        tape_offset,
        min,
        max,
    } = mbs[idx]
    else {
        unreachable!()
    };
    let range = max - min + 1;
    let current_byte = tape[tape_offset];
    let current_count = min + (current_byte as u32 % range);

    let new_count = if rng.random_bool(0.5) {
        if current_count < max {
            current_count + 1
        } else {
            current_count - 1
        }
    } else if current_count > min {
        current_count - 1
    } else {
        current_count + 1
    };

    if new_count < min || new_count > max {
        return false;
    }

    let offset = new_count - min;
    let base = current_byte.wrapping_sub(current_byte % range as u8);
    tape[tape_offset] = base.wrapping_add(offset as u8);
    true
}

// ── Top-level entry point ──

/// Apply a weighted-random mutation to the tape. Returns which kind was applied.
///
/// If the chosen operator fails (e.g., no matching fragments for splice), falls back to
/// `point_mutate` which always succeeds.
pub fn mutate(
    tape: &mut Vec<u8>,
    meta: &MutationMeta,
    grammar: &GrammarIr,
    profile: &Profile,
    db: &FragmentDb,
    rng: &mut impl Rng,
) -> MutationKind {
    // Weight rationale: point mutations and range re-randomize are cheap O(1) ops, so they get
    // the highest combined weight (3+2=5). Structure-aware ops (subtree_regenerate, splice) are
    // more expensive but more targeted at finding interesting coverage, so they share a moderate
    // weight (2 each). Toggle/perturb are low-weight (1 each) because they make minimal changes
    // that are most useful for fine-tuning near a coverage frontier.
    //
    // Weights: point_mutate=3, range_rerandomize=2, splice=2,
    //          subtree_regenerate=2, toggle_optional=1, perturb_repetition=1
    let roll = rng.random_range(0..11u32);

    let kind = match roll {
        0..=2 => MutationKind::PointMutate,
        3..=4 => MutationKind::RangeRerandomize,
        5..=6 => MutationKind::Splice,
        7..=8 => MutationKind::SubtreeRegenerate,
        9 => MutationKind::ToggleOptional,
        10 => MutationKind::PerturbRepetition,
        _ => unreachable!(),
    };

    let success = match kind {
        MutationKind::PointMutate => {
            point_mutate(tape, rng);
            true
        }
        MutationKind::RangeRerandomize => {
            range_rerandomize(tape, meta, rng);
            true
        }
        MutationKind::Splice => splice(tape, meta, db, rng),
        MutationKind::SubtreeRegenerate => subtree_regenerate(tape, meta, grammar, profile, rng),
        MutationKind::ToggleOptional => toggle_optional(tape, meta, rng),
        MutationKind::PerturbRepetition => perturb_repetition(tape, meta, rng),
    };

    if success {
        kind
    } else {
        point_mutate(tape, rng);
        MutationKind::PointMutate
    }
}
