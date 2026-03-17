# barkus-viz

Generates coverage reports from a grammar and a generated corpus. Shows which productions and alternatives are exercised, distribution of depth/node counts, highlights hard-to-reach parts of the grammar, and suggests profile flag changes to reduce failure rates.

## Usage

```bash
cargo run -p barkus-viz -- <grammar> [options]
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `<grammar>` | Path to grammar file (.ebnf, .g4, .peg) | required |
| `-n`, `--count` | Number of payloads to generate | 10000 |
| `--format` | Output format: `text`, `html`, or `json` | text |
| `-o`, `--output` | Output HTML file (only with `--format=html`) | coverage.html |
| `--seed` | RNG seed (omit for random) | random |
| `--max-depth` | Max AST depth | 20 |
| `--max-nodes` | Max total AST nodes | 10000 |
| `--start` | Start rule name | grammar default |
| `--no-open` | Don't open the report in a browser (only with `--format=html`) | opens automatically |

### Examples

```bash
# Text report to stdout (default)
cargo run --release -p barkus-viz -- fixtures/grammars/json.ebnf -n 100000

# HTML report
cargo run --release -p barkus-viz -- fixtures/grammars/json.ebnf -n 100000 --format=html -o report.html

# JSON report
cargo run --release -p barkus-viz -- fixtures/grammars/json.ebnf -n 100000 --format=json

# With custom limits
cargo run --release -p barkus-viz -- fixtures/grammars/json.ebnf -n 100000 --max-depth 40 --max-nodes 50000
```

## Report sections

- **Summary banner** — total payloads, failure rate (with depth/node breakdown), production coverage %
- **Recommendations** — when failure rate exceeds 10%, suggests concrete `--max-depth` / `--max-nodes` flag values to reduce failures, with a copy-pasteable full command
- **Depth / node count histograms** — distribution charts
- **Production table** — sortable, with expandable per-alternative breakdowns
- **Grammar treemap** — sized by hit count, colored by coverage (HTML only)
- **Hard-to-reach panel** — unreached productions, starved alternatives, choke points, weight-disadvantaged alternatives
