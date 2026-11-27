pub mod map;

use rand::{Rng, RngExt};

use crate::profile::ValidityMode;

/// Control header size in bytes.
const HEADER_SIZE: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionTape {
    pub bytes: Vec<u8>,
}

impl DecisionTape {
    pub fn new(validity_mode: ValidityMode) -> Self {
        let mode_byte = match validity_mode {
            ValidityMode::Strict => 0,
            ValidityMode::NearValid => 1,
            ValidityMode::Havoc => 2,
        };
        Self {
            bytes: vec![mode_byte, 0], // mode + reserved
        }
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn validity_mode(&self) -> ValidityMode {
        if self.bytes.is_empty() {
            return ValidityMode::Strict;
        }
        match self.bytes[0] {
            1 => ValidityMode::NearValid,
            2 => ValidityMode::Havoc,
            _ => ValidityMode::Strict,
        }
    }
}

pub struct TapeReader<'a> {
    tape: &'a [u8],
    offset: usize,
}

impl<'a> TapeReader<'a> {
    pub fn new(tape: &'a [u8]) -> Self {
        Self {
            tape,
            offset: HEADER_SIZE,
        }
    }

    /// Read one byte. Returns 0 if tape exhausted (total decoder).
    pub fn read_byte(&mut self) -> u8 {
        if self.offset < self.tape.len() {
            let b = self.tape[self.offset];
            self.offset += 1;
            b
        } else {
            0
        }
    }

    /// Read two bytes as a little-endian `u16`. Returns 0 for past-EOF bytes (total decoder).
    pub fn read_u16(&mut self) -> u16 {
        let lo = self.read_byte();
        let hi = self.read_byte();
        u16::from_le_bytes([lo, hi])
    }

    /// Read four bytes as a little-endian `u32`. Returns 0 for past-EOF bytes (total decoder).
    pub fn read_u32(&mut self) -> u32 {
        let b0 = self.read_byte();
        let b1 = self.read_byte();
        let b2 = self.read_byte();
        let b3 = self.read_byte();
        u32::from_le_bytes([b0, b1, b2, b3])
    }

    /// Choose among N alternatives. Returns index in `0..n`.
    ///
    /// Decision width is fixed by `n` (ADR-0002):
    /// - `n <= 1`: 0 bytes
    /// - `2..=256`: 1 byte
    /// - `257..=65536`: 2 bytes (little-endian `u16`)
    /// - `n > 65536`: 4 bytes (little-endian `u32`)
    pub fn choose(&mut self, n: usize) -> usize {
        if n <= 1 {
            return 0;
        }
        if n <= 256 {
            self.read_byte() as usize % n
        } else if n <= 65_536 {
            self.read_u16() as usize % n
        } else {
            (u64::from(self.read_u32()) % (n as u64)) as usize
        }
    }

    /// Choose repetition count in `[min, max]`.
    ///
    /// Decision width is fixed by `range = max - min + 1` (ADR-0002):
    /// - `range <= 1` (`min >= max`): 0 bytes
    /// - `2..=256`: 1 byte
    /// - `257..=65536`: 2 bytes (little-endian `u16`)
    /// - `range > 65536`: 4 bytes (little-endian `u32`)
    pub fn repetition(&mut self, min: u32, max: u32) -> u32 {
        if min >= max {
            return min;
        }
        let range = u64::from(max - min) + 1;
        let offset = if range <= 256 {
            u64::from(self.read_byte()) % range
        } else if range <= 65_536 {
            u64::from(self.read_u16()) % range
        } else {
            u64::from(self.read_u32()) % range
        };
        min + offset as u32
    }

    /// Current offset (for TapeMap).
    pub fn offset(&self) -> usize {
        self.offset
    }
}

pub struct TapeWriter {
    bytes: Vec<u8>,
}

impl TapeWriter {
    pub fn new(validity_mode: ValidityMode) -> Self {
        let mode_byte = match validity_mode {
            ValidityMode::Strict => 0,
            ValidityMode::NearValid => 1,
            ValidityMode::Havoc => 2,
        };
        Self {
            bytes: vec![mode_byte, 0],
        }
    }

    /// Write a choice decision. Encodes `chosen` such that `TapeReader::choose(n)` returns
    /// `chosen`, using the same fixed decision width as [`TapeReader::choose`].
    pub fn write_choice(&mut self, chosen: usize, n: usize, rng: &mut impl Rng) {
        if n <= 1 {
            return;
        }
        debug_assert!(chosen < n);
        if n <= 256 {
            self.bytes.push(encode_residue_byte(chosen, n, rng));
        } else if n <= 65_536 {
            let v = encode_residue_u16(chosen, n, rng);
            self.bytes.extend_from_slice(&v.to_le_bytes());
        } else {
            let v = encode_residue_u32(chosen as u64, n as u64, rng);
            self.bytes.extend_from_slice(&v.to_le_bytes());
        }
    }

    /// Write a repetition decision. Encodes `count` such that `TapeReader::repetition(min, max)`
    /// returns `count`, using the same fixed decision width as [`TapeReader::repetition`].
    pub fn write_repetition(&mut self, count: u32, min: u32, max: u32, rng: &mut impl Rng) {
        if min >= max {
            return;
        }
        debug_assert!((min..=max).contains(&count));
        let range = u64::from(max - min) + 1;
        let offset = u64::from(count - min);
        if range <= 256 {
            self.bytes
                .push(encode_residue_byte(offset as usize, range as usize, rng));
        } else if range <= 65_536 {
            let v = encode_residue_u16(offset as usize, range as usize, rng);
            self.bytes.extend_from_slice(&v.to_le_bytes());
        } else {
            let v = encode_residue_u32(offset, range, rng);
            self.bytes.extend_from_slice(&v.to_le_bytes());
        }
    }

    /// Current offset (for TapeMap).
    pub fn offset(&self) -> usize {
        self.bytes.len()
    }

    pub fn finish(self) -> DecisionTape {
        DecisionTape { bytes: self.bytes }
    }
}

/// Sample a byte `b ∈ [0, 256)` uniformly among those with `b % n == chosen`.
///
/// Requires `2 ≤ n ≤ 256` and `chosen < n`. u16 arithmetic so values of
/// `n` that don't divide 256 don't wrap chosen onto a neighbouring residue.
pub(crate) fn encode_residue_byte(chosen: usize, n: usize, rng: &mut impl Rng) -> u8 {
    debug_assert!((2..=256).contains(&n) && chosen < n);
    if n == 256 {
        return chosen as u8;
    }
    let n = n as u16;
    let chosen = chosen as u16;
    let count = 256u16 / n + u16::from(chosen < 256u16 % n);
    let k = rng.random_range(0..count);
    (chosen + n * k) as u8
}

/// Fixed byte width of a repetition decision for `[min, max]`.
#[inline]
pub(crate) fn repetition_width(min: u32, max: u32) -> usize {
    if min >= max {
        return 0;
    }
    let range = u64::from(max - min) + 1;
    if range <= 256 {
        1
    } else if range <= 65_536 {
        2
    } else {
        4
    }
}

/// Sample a `u16` `v ∈ [0, 65536)` uniformly among those with `v % n == chosen`.
///
/// Requires `257 ≤ n ≤ 65536` and `chosen < n`. Uses `u32` arithmetic so values
/// of `n` that don't divide 65536 don't wrap `chosen` onto a neighbouring residue.
pub(crate) fn encode_residue_u16(chosen: usize, n: usize, rng: &mut impl Rng) -> u16 {
    debug_assert!((257..=65_536).contains(&n) && chosen < n);
    if n == 65_536 {
        return chosen as u16;
    }
    let n = n as u32;
    let chosen = chosen as u32;
    let count = 65_536u32 / n + u32::from(chosen < 65_536u32 % n);
    let k = rng.random_range(0..count);
    (chosen + n * k) as u16
}

/// Sample a `u32` `v ∈ [0, 2^32)` uniformly among those with `v % n == chosen`.
///
/// Requires `65537 ≤ n ≤ 2^32` and `chosen < n`. Uses `u64` arithmetic so values
/// of `n` that don't divide `2^32` don't wrap `chosen` onto a neighbouring residue.
pub(crate) fn encode_residue_u32(chosen: u64, n: u64, rng: &mut impl Rng) -> u32 {
    const MOD: u64 = 1u64 << 32;
    debug_assert!((65_537..=MOD).contains(&n) && chosen < n);
    if n == MOD {
        return chosen as u32;
    }
    let count = MOD / n + u64::from(chosen < MOD % n);
    let k = rng.random_range(0..count);
    (chosen + n * k) as u32
}
