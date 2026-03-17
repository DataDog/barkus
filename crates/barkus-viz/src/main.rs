use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process;

use barkus_core::generate;
use barkus_core::ir::GrammarIr;
use barkus_core::profile::Profile;
use clap::Parser;
use rand::rngs::SmallRng;
use rand::SeedableRng;

use barkus_viz::html;
use barkus_viz::json;
use barkus_viz::reachability;
use barkus_viz::recommend;
use barkus_viz::stats::StatsCollector;
use barkus_viz::text;

#[derive(Clone, Copy, Default, clap::ValueEnum)]
enum Format {
    /// Plain text to stdout (default)
    #[default]
    Text,
    /// HTML report file
    Html,
    /// JSON to stdout
    Json,
}

#[derive(Parser)]
#[command(name = "barkus-viz", about = "Generate a coverage report from a grammar + corpus")]
struct Cli {
    /// Path to the grammar file (.ebnf, .g4, .peg)
    grammar: PathBuf,

    /// Number of payloads to generate
    #[arg(short = 'n', long, default_value_t = 10_000)]
    count: u64,

    /// Output format
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,

    /// Output HTML file path (only used with --format=html)
    #[arg(short, long, default_value = "coverage.html")]
    output: PathBuf,

    /// RNG seed (random if omitted)
    #[arg(long)]
    seed: Option<u64>,

    /// Maximum AST depth
    #[arg(long)]
    max_depth: Option<u32>,

    /// Maximum total AST nodes
    #[arg(long)]
    max_nodes: Option<u32>,

    /// Start rule name
    #[arg(long)]
    start: Option<String>,

    /// Don't open the report in a browser (only used with --format=html)
    #[arg(long)]
    no_open: bool,
}

fn main() {
    let cli = Cli::parse();

    let (grammar, profile) = compile_and_configure(&cli.grammar, cli.start.as_deref(), cli.max_depth, cli.max_nodes);

    let seed = cli.seed.unwrap_or_else(|| rand::random());
    let mut rng = SmallRng::seed_from_u64(seed);

    let is_tty = io::stderr().is_terminal();

    let mut collector = StatsCollector::new_with_capacity(&grammar, cli.count);
    let spinner = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

    for i in 0..cli.count {
        if is_tty && i % 1_000 == 0 {
            let frame = spinner[(i as usize / 1_000) % spinner.len()];
            let _ = write!(io::stderr(), "\r\x1b[2K  {frame} {i} / {} payloads generated", cli.count);
            let _ = io::stderr().flush();
        } else if !is_tty && i > 0 && i % 10_000 == 0 {
            eprintln!("  {} / {} payloads generated", i, cli.count);
        }
        match generate::generate(&grammar, &profile, &mut rng) {
            Ok((ast, tape, tape_map)) => {
                collector.record_payload(&ast, &tape, &tape_map);
            }
            Err(e) => {
                collector.record_failure(&e);
            }
        }
    }

    if is_tty {
        eprint!("\r\x1b[2K  ✓ {} / {} payloads generated\n", cli.count, cli.count);
    } else {
        eprintln!("  {} / {} payloads generated — done.", cli.count, cli.count);
    }

    let stats = collector.finalize(&grammar);
    let reachability = reachability::analyze(&grammar, &stats);
    let recommendations = recommend::analyze(&stats, &profile);
    let grammar_path = cli.grammar.display().to_string();

    match cli.format {
        Format::Text => {
            let report = text::render(&stats, &reachability, &recommendations, &grammar_path);
            print!("{report}");
        }
        Format::Json => {
            let report = json::render(&stats, &reachability, &recommendations, &grammar_path);
            println!("{report}");
        }
        Format::Html => {
            let report = html::render(&stats, &reachability, &recommendations, &grammar_path);

            fs::write(&cli.output, report).unwrap_or_else(|e| {
                eprintln!("error: failed to write {}: {}", cli.output.display(), e);
                process::exit(1);
            });

            eprintln!("Report written to {}", cli.output.display());

            if !cli.no_open {
                let path = fs::canonicalize(&cli.output).unwrap_or_else(|_| cli.output.clone());
                if let Err(e) = opener::open(&path) {
                    eprintln!("warning: could not open browser: {}", e);
                }
            }
        }
    }
}

fn compile_grammar(path: &std::path::Path, source: &str) -> Result<GrammarIr, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("ebnf");
    match ext {
        "g4" => barkus_antlr::compile(source).map_err(|e| e.to_string()),
        "peg" => barkus_peg::compile(source).map_err(|e| e.to_string()),
        _ => barkus_ebnf::compile(source).map_err(|e| e.to_string()),
    }
}

fn compile_and_configure(
    path: &PathBuf,
    start: Option<&str>,
    max_depth: Option<u32>,
    max_nodes: Option<u32>,
) -> (GrammarIr, Profile) {
    let source = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error: cannot read {}: {}", path.display(), e);
        process::exit(1);
    });

    let mut grammar = compile_grammar(path, &source).unwrap_or_else(|e| {
        eprintln!("error: failed to compile grammar: {}", e);
        process::exit(1);
    });

    if let Some(start_name) = start {
        match grammar.productions.iter().find(|p| p.name == *start_name) {
            Some(p) => grammar.start = p.id,
            None => {
                eprintln!("error: no rule named {:?}", start_name);
                process::exit(1);
            }
        }
    }

    let mut builder = Profile::builder();
    if let Some(d) = max_depth {
        builder = builder.max_depth(d);
    }
    if let Some(n) = max_nodes {
        builder = builder.max_total_nodes(n);
    }
    let profile = builder.build();

    (grammar, profile)
}
