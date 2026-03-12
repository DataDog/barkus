use barkus_core::generate::generate;
use barkus_core::ir::GrammarIr;
use barkus_core::profile::Profile;
use rand::rngs::SmallRng;
use rand::SeedableRng;

pub fn generate_one(ir: &GrammarIr) -> Vec<u8> {
    generate_seeded(ir, 42)
}

pub fn generate_seeded(ir: &GrammarIr, seed: u64) -> Vec<u8> {
    let profile = Profile::default();
    let mut rng = SmallRng::seed_from_u64(seed);
    let (ast, _tape, _map) = generate(ir, &profile, &mut rng).unwrap();
    ast.serialize()
}
