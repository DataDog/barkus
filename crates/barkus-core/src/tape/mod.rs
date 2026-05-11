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

    /// Choose among N alternatives. Returns index in 0..n.
    pub fn choose(&mut self, n: usize) -> usize {
        if n <= 1 {
            return 0;
        }
        let b = self.read_byte() as usize;
        b % n
    }

    /// Choose repetition count in [min, max].
    pub fn repetition(&mut self, min: u32, max: u32) -> u32 {
        if min >= max {
            return min;
        }
        let range = max - min + 1;
        let b = self.read_byte() as u32;
        min + (b % range)
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

    /// Write a choice decision. Encodes `chosen` such that `chosen == byte % n`.
    pub fn write_choice(&mut self, chosen: usize, n: usize, rng: &mut impl Rng) {
        if n <= 1 {
            return;
        }
        if n >= 256 {
            self.bytes.push(chosen as u8);
            return;
        }
        self.bytes.push(encode_residue_byte(chosen, n, rng));
    }

    /// Write a repetition decision. Encodes `count` such that `count == min + byte % range`.
    pub fn write_repetition(&mut self, count: u32, min: u32, max: u32, rng: &mut impl Rng) {
        if min >= max {
            return;
        }
        let range = (max - min + 1) as usize;
        let offset = (count - min) as usize;
        self.bytes.push(encode_residue_byte(offset, range, rng));
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
    let n = n as u16;
    let chosen = chosen as u16;
    let count = 256u16 / n + u16::from(chosen < 256u16 % n);
    let k = rng.random_range(0..count);
    (chosen + n * k) as u8
}
