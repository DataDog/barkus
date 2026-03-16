use barkus_core::profile::{Profile, ValidityMode};

#[test]
fn profile_defaults() {
    let p = Profile::default();
    assert_eq!(p.max_depth, 20);
    assert_eq!(p.max_total_nodes, 10_000);
    assert_eq!(p.repetition_bounds, (0, 5));
    assert_eq!(p.validity_mode, ValidityMode::Strict);
    assert!((p.havoc_intensity - 0.5).abs() < f32::EPSILON);
    assert!(p.dictionary.is_empty());
    assert!(p.rule_overrides.is_empty());
}

#[test]
fn profile_builder() {
    let p = Profile::builder()
        .max_depth(12)
        .max_total_nodes(5000)
        .validity_mode(ValidityMode::Havoc)
        .repetition_bounds(1, 3)
        .havoc_intensity(0.8)
        .build();

    assert_eq!(p.max_depth, 12);
    assert_eq!(p.max_total_nodes, 5000);
    assert_eq!(p.validity_mode, ValidityMode::Havoc);
    assert_eq!(p.repetition_bounds, (1, 3));
}

#[test]
fn profile_json_deser() {
    let json = r#"{
        "validity_mode": "NearValid",
        "max_depth": 15,
        "max_total_nodes": 8000,
        "repetition_bounds": [1, 4],
        "dictionary": [[104, 101, 108, 108, 111]],
        "havoc_intensity": 0.3,
        "rule_overrides": {}
    }"#;

    let p: Profile = serde_json::from_str(json).unwrap();
    assert_eq!(p.validity_mode, ValidityMode::NearValid);
    assert_eq!(p.max_depth, 15);
    assert_eq!(p.dictionary.len(), 1);
    assert_eq!(p.dictionary[0], b"hello");
}
