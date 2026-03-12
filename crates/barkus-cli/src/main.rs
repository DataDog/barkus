use std::fs;
use std::path::PathBuf;
use std::process;

use barkus_core::generate::generate;
use barkus_core::ir::GrammarIr;
use barkus_core::profile::Profile;
use clap::Parser;
use rand::rngs::SmallRng;
use rand::SeedableRng;

#[derive(Parser)]
#[command(name = "barkus-cli", about = "Generate samples from a grammar (EBNF, ANTLR v4, PEG)")]
struct Cli {
    /// Path to the grammar file (.ebnf, .g4, .peg)
    grammar: PathBuf,

    /// Number of samples to generate
    #[arg(long, default_value_t = 10)]
    count: u32,

    /// Fixed random seed (random by default)
    #[arg(long)]
    seed: Option<u64>,

    /// Maximum derivation depth
    #[arg(long)]
    max_depth: Option<u32>,

    /// Override start rule name (default: auto-detect)
    #[arg(long)]
    start: Option<String>,
}

fn main() {
    let cli = Cli::parse();

    let source = fs::read_to_string(&cli.grammar).unwrap_or_else(|e| {
        eprintln!("error reading {}: {e}", cli.grammar.display());
        process::exit(1);
    });

    let mut grammar = compile_grammar(&cli.grammar, &source).unwrap_or_else(|e| {
        eprintln!("parse error: {e}");
        process::exit(1);
    });

    if let Some(ref start_name) = cli.start {
        match grammar
            .productions
            .iter()
            .find(|p| p.name == *start_name)
        {
            Some(p) => grammar.start = p.id,
            None => {
                eprintln!("error: no rule named {:?}", start_name);
                process::exit(1);
            }
        }
    }

    let profile = match cli.max_depth {
        Some(d) => Profile::builder().max_depth(d).build(),
        None => Profile::default(),
    };

    let mut rng: SmallRng = match cli.seed {
        Some(s) => SmallRng::seed_from_u64(s),
        None => SmallRng::from_entropy(),
    };

    for _ in 0..cli.count {
        match generate(&grammar, &profile, &mut rng) {
            Ok((ast, _tape)) => {
                let bytes = ast.serialize();
                match String::from_utf8(bytes) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        let lossy = String::from_utf8_lossy(e.as_bytes());
                        println!("{lossy}");
                    }
                }
            }
            Err(e) => {
                eprintln!("generate error: {e}");
            }
        }
    }
}

fn compile_grammar(path: &std::path::Path, source: &str) -> Result<GrammarIr, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("ebnf");

    match ext {
        "g4" => barkus_antlr::compile(source).map_err(|e| e.to_string()),
        "peg" => barkus_peg::compile(source).map_err(|e| e.to_string()),
        _ => barkus_ebnf::compile(source).map_err(|e| e.to_string()),
    }
}
