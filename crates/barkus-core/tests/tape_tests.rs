use barkus_core::profile::ValidityMode;
use barkus_core::tape::{DecisionTape, TapeReader, TapeWriter};
use rand::rngs::SmallRng;
use rand::SeedableRng;

#[test]
fn tape_reader_totality_empty() {
    // Any &[u8] input produces valid decisions, never panics.
    let tape = DecisionTape::new(ValidityMode::Strict);
    let mut reader = TapeReader::new(&tape.bytes);

    // Reading beyond tape returns 0.
    assert_eq!(reader.read_byte(), 0);
    assert_eq!(reader.choose(5), 0);
    assert_eq!(reader.repetition(2, 8), 2); // min when byte is 0
}

#[test]
fn tape_reader_totality_random_bytes() {
    // Arbitrary bytes never cause panics.
    let bytes: Vec<u8> = (0..=255).collect();
    let mut reader = TapeReader::new(&bytes);

    for _ in 0..300 {
        let _ = reader.read_byte();
    }

    let mut reader2 = TapeReader::new(&bytes);
    for n in 1..=20 {
        let choice = reader2.choose(n);
        assert!(choice < n);
    }
}

#[test]
fn tape_writer_reader_roundtrip_choice() {
    let mut rng = SmallRng::seed_from_u64(42);
    let mut writer = TapeWriter::new(ValidityMode::Strict);

    let decisions: Vec<(usize, usize)> = vec![(0, 3), (2, 3), (1, 5), (4, 5), (0, 2), (1, 2)];

    for &(chosen, n) in &decisions {
        writer.write_choice(chosen, n, &mut rng);
    }

    let tape = writer.finish();
    let mut reader = TapeReader::new(&tape.bytes);

    for &(expected, n) in &decisions {
        let got = reader.choose(n);
        assert_eq!(got, expected, "choice mismatch for n={n}");
    }
}

#[test]
fn tape_writer_reader_roundtrip_repetition() {
    let mut rng = SmallRng::seed_from_u64(99);
    let mut writer = TapeWriter::new(ValidityMode::Strict);

    let decisions: Vec<(u32, u32, u32)> = vec![(2, 0, 5), (0, 0, 3), (5, 3, 8), (3, 3, 3)];

    for &(count, min, max) in &decisions {
        writer.write_repetition(count, min, max, &mut rng);
    }

    let tape = writer.finish();
    let mut reader = TapeReader::new(&tape.bytes);

    for &(expected, min, max) in &decisions {
        let got = reader.repetition(min, max);
        assert_eq!(got, expected, "repetition mismatch for [{min},{max}]");
    }
}

#[test]
fn tape_header_preserves_validity_mode() {
    for mode in [
        ValidityMode::Strict,
        ValidityMode::NearValid,
        ValidityMode::Havoc,
    ] {
        let tape = DecisionTape::new(mode);
        assert_eq!(tape.validity_mode(), mode);
    }
}

#[test]
fn tape_from_bytes_empty_defaults_strict() {
    let tape = DecisionTape::from_bytes(vec![]);
    assert_eq!(tape.validity_mode(), ValidityMode::Strict);
}

#[test]
fn tape_wide_choice_roundtrip_and_widths() {
    let mut rng = SmallRng::seed_from_u64(123);
    let cases: &[(usize, usize, usize)] = &[
        (0, 1, 0),
        (0, 256, 1),
        (255, 256, 1),
        (0, 257, 2),
        (256, 257, 2),
        (0, 452, 2),
        (256, 452, 2),
        (451, 452, 2),
        (65_535, 65_536, 2),
        (0, 70_000, 4),
        (65_536, 70_000, 4),
        (69_999, 70_000, 4),
    ];

    let mut writer = TapeWriter::new(ValidityMode::Strict);
    for &(chosen, n, expected_width) in cases {
        let before = writer.offset();
        writer.write_choice(chosen, n, &mut rng);
        assert_eq!(
            writer.offset() - before,
            expected_width,
            "wrong width for n={n}"
        );
    }

    let tape = writer.finish();
    let mut reader = TapeReader::new(&tape.bytes);
    for &(expected, n, expected_width) in cases {
        let before = reader.offset();
        let got = reader.choose(n);
        assert_eq!(got, expected, "choice mismatch for n={n}");
        assert_eq!(
            reader.offset() - before,
            expected_width,
            "reader width mismatch for n={n}"
        );
    }
}

#[test]
fn tape_wide_repetition_roundtrip_and_u32_max() {
    let mut rng = SmallRng::seed_from_u64(456);
    let cases: &[(u32, u32, u32, usize)] = &[
        (255, 0, 255, 1),
        (0, 0, 300, 2),
        (256, 0, 300, 2),
        (300, 0, 300, 2),
        (10, 10, 70_000, 4),
        (69_999, 10, 70_000, 4),
        (0, 0, u32::MAX, 4),
        (u32::MAX, 0, u32::MAX, 4),
    ];

    let mut writer = TapeWriter::new(ValidityMode::Strict);
    for &(count, min, max, expected_width) in cases {
        let before = writer.offset();
        writer.write_repetition(count, min, max, &mut rng);
        assert_eq!(
            writer.offset() - before,
            expected_width,
            "wrong width for [{min}, {max}]"
        );
    }

    let tape = writer.finish();
    let mut reader = TapeReader::new(&tape.bytes);
    for &(expected, min, max, expected_width) in cases {
        let before = reader.offset();
        let got = reader.repetition(min, max);
        assert_eq!(got, expected, "repetition mismatch for [{min}, {max}]");
        assert_eq!(
            reader.offset() - before,
            expected_width,
            "reader width mismatch for [{min}, {max}]"
        );
    }
}

#[test]
fn tape_reader_wide_totality_on_truncated_tape() {
    // 2-byte header + 1 body byte (0x2a): past-EOF bytes read as 0.
    let bytes = [0, 0, 0x2a];
    let mut reader = TapeReader::new(&bytes);
    assert_eq!(reader.read_u16(), 0x002a);
    assert_eq!(reader.read_u16(), 0);
    assert_eq!(reader.read_u32(), 0);
    assert_eq!(reader.choose(452), 0);
    assert_eq!(reader.repetition(5, 305), 5);
}
