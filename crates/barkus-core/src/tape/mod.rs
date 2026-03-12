pub mod map;

use rand::Rng;

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
        // Pick a random byte that maps to `chosen` via `byte % n`.
        let base = rng.gen_range(0u8..=255);
        let byte = base - (base % n as u8) + chosen as u8;
        self.bytes.push(byte);
    }

    /// Write a repetition decision. Encodes `count` such that `count == min + byte % range`.
    pub fn write_repetition(&mut self, count: u32, min: u32, max: u32, rng: &mut impl Rng) {
        if min >= max {
            return;
        }
        let range = max - min + 1;
        let offset = count - min;
        let base = rng.gen_range(0u8..=255);
        let byte = base - (base % range as u8) + offset as u8;
        self.bytes.push(byte);
    }

    pub fn finish(self) -> DecisionTape {
        DecisionTape { bytes: self.bytes }
    }
}
