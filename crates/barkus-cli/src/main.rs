use std::fs;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::process;

use barkus_core::ast::Ast;
use barkus_core::generate::{decode, generate};
use barkus_core::ir::GrammarIr;
use barkus_core::profile::Profile;
use clap::{Parser, Subcommand};
use rand::rngs::SmallRng;
use rand::SeedableRng;

#[derive(Parser)]
#[command(name = "barkus-cli", about = "Generate samples from a grammar (EBNF, ANTLR v4, PEG)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate random samples from a grammar
    Generate {
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

        /// Emit hex-encoded decision tapes to stderr (one per sample)
        #[arg(long)]
        emit_tape: bool,
    },
    /// Decode a sample from a hex-encoded decision tape
    Decode {
        /// Path to the grammar file (.ebnf, .g4, .peg)
        grammar: PathBuf,

        /// Hex-encoded decision tape (use "-" to read from stdin)
        #[arg(long)]
        tape: String,

        /// Maximum derivation depth
        #[arg(long)]
        max_depth: Option<u32>,

        /// Override start rule name (default: auto-detect)
        #[arg(long)]
        start: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Generate {
            grammar,
            count,
            seed,
            max_depth,
            start,
            emit_tape,
        } => {
            let (ir, profile) = compile_and_configure(&grammar, start.as_deref(), max_depth);

            let mut rng: SmallRng = match seed {
                Some(s) => SmallRng::seed_from_u64(s),
                None => rand::make_rng(),
            };

            for _ in 0..count {
                match generate(&ir, &profile, &mut rng) {
                    Ok((ast, tape, _map)) => {
                        print_output(&ast);
                        if emit_tape {
                            eprintln!("{}", hex::encode(&tape.bytes));
                        }
                    }
                    Err(e) => {
                        eprintln!("generate error: {e}");
                    }
                }
            }
        }
        Command::Decode {
            grammar,
            tape,
            max_depth,
            start,
        } => {
            let (ir, profile) = compile_and_configure(&grammar, start.as_deref(), max_depth);

            let hex_str = if tape == "-" {
                let stdin = io::stdin();
                stdin
                    .lock()
                    .lines()
                    .next()
                    .unwrap_or_else(|| {
                        eprintln!("error: no input on stdin");
                        process::exit(1);
                    })
                    .unwrap_or_else(|e| {
                        eprintln!("error reading stdin: {e}");
                        process::exit(1);
                    })
            } else {
                tape
            };

            let tape_bytes = hex::decode(hex_str.trim()).unwrap_or_else(|e| {
                eprintln!("error: invalid hex string: {e}");
                process::exit(1);
            });

            match decode(&ir, &profile, &tape_bytes) {
                Ok((ast, _map)) => print_output(&ast),
                Err(e) => {
                    eprintln!("decode error: {e}");
                    process::exit(1);
                }
            }
        }
    }
}

fn compile_and_configure(path: &PathBuf, start: Option<&str>, max_depth: Option<u32>) -> (GrammarIr, Profile) {
    let source = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {e}", path.display());
        process::exit(1);
    });

    let mut grammar = compile_grammar(path, &source).unwrap_or_else(|e| {
        eprintln!("parse error: {e}");
        process::exit(1);
    });

    if let Some(start_name) = start {
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

    let profile = match max_depth {
        Some(d) => Profile::builder().max_depth(d).build(),
        None => Profile::default(),
    };

    (grammar, profile)
}

fn print_output(ast: &Ast) {
    let bytes = ast.serialize();
    match String::from_utf8(bytes) {
        Ok(s) => println!("{s}"),
        Err(e) => {
            let lossy = String::from_utf8_lossy(e.as_bytes());
            println!("{lossy}");
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
