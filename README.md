# Barkus

Structure-aware fuzzer that generates structured inputs from grammars (EBNF, ANTLR v4, PEG). Reuse your existing parser grammar to fuzz-test parsers, protocols, and file formats. The [ANTLR grammars-v4](https://github.com/antlr/grammars-v4) repository has ready-made grammars for hundreds of languages and formats.

## How it works

Barkus compiles a grammar into a normalized intermediate representation, then walks it to produce random valid outputs. Every generation decision (which alternative to pick, how many repetitions, which character in a class) is recorded onto a **decision tape** — a flat byte sequence where each decision is exactly one byte.

This design is **fuzzer-friendly**: a single byte flip in the tape changes one structural decision without scrambling the rest of the output. Traditional byte-level fuzzing of grammar generators suffers from the *havoc paradox* — variable-width byte consumption means one mutation cascades into a completely different parse tree. Fixed-width tape encoding solves this: mutators like AFL and libFuzzer can operate directly on the tape with high locality.

The approach draws on research in grammar-aware fuzzing:

- Aschermann et al., [Nautilus: Fishing for Deep Bugs with Grammars](https://www.syssec.ruhr-uni-bochum.de/media/emma/veroeffentlichungen/2019/02/12/2019-NDSS.pdf) (NDSS 2019) — tree-based mutations on a normalized grammar IR
- Srivastava et al., [Gramatron: Effective Grammar-Aware Fuzzing](https://doi.org/10.1145/3460319.3464814) (ISSTA 2021) — depth-aware alternative selection to avoid structural bias
- Blazytko et al., [GRIMOIRE: Synthesizing Structure while Fuzzing](https://www.usenix.org/conference/usenixsecurity19/presentation/blazytko) (USENIX Security 2019) — structure synthesis from byte-level mutations
- Padhye et al., [Semantic Fuzzing with Zest](https://doi.org/10.1145/3293882.3330576) (ISSTA 2019) — parametric fuzzing with byte-to-structure locality
- Liyanage et al., [Zeugma: Parametric Fuzzing with Structure-Aware Crossover](https://doi.org/10.1145/3597926.3598040) (ISSTA 2023) — structure-aware crossover on decision streams
- Wang et al., [Skyfire: Data-Driven Seed Generation for Fuzzing](https://doi.org/10.1109/SP.2017.23) (IEEE S&P 2017) — corpus-mined subtree splicing
- Holler et al., [Fuzzing with Code Fragments](https://www.usenix.org/conference/usenixsecurity12/technical-sessions/presentation/holler) (USENIX Security 2012) — fragment recombination from existing corpora

**Use it as:**

- **Rust library** (`barkus-core`) — embed generation, decoding, and mutation in your own tooling. Sans I/O: no file access, no global state, caller provides the RNG.
- **Go library** (`go/pkg/barkus`) — CGo bindings for `go test -fuzz` integration. Feed the fuzzer's `[]byte` corpus entries as decision tapes and decode them into structured grammar outputs.
- **CLI** (`barkus-cli` / `barkus-gen`) — generate samples from the command line for quick prototyping, corpus seeding, or scripted pipelines.

## Quick start

### Build

```bash
# Rust CLI
cargo build -p barkus-cli --release

# Go CLI (builds the FFI library first)
make go-example
```

### Example grammar

Point barkus at any EBNF, ANTLR v4, or PEG grammar. Here's a simple JSON example in EBNF (`fixtures/grammars/json.ebnf`):

```ebnf
start = value ;

value = object | array | string | number | "true" | "false" | "null" ;

object = "{" "}" | "{" members "}" ;
members = pair | pair "," members ;
pair = string ":" value ;

array = "[" "]" | "[" elements "]" ;
elements = value | value "," elements ;

string = "\"" chars "\"" ;
chars = char | char chars ;
char = "a" | "b" | "c" | "d" | "e" | "f" | "x" | "y" | "z"
     | "0" | "1" | "2" | "3" ;

number = digits | "-" digits | digits "." digits ;
digits = digit | digit digits ;
digit = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;
```

### Generate with the Rust CLI

```bash
$ cargo run -p barkus-cli -- fixtures/grammars/json.ebnf --count 5 --seed 42
"d"
null
"fdxx"
true
true
```

```bash
$ cargo run -p barkus-cli -- fixtures/grammars/url.ebnf --count 3 --seed 42
ftp://o/?g=l&i=q&k=l&g=d&j=d&l=l
https://d.h:7/e/r1g/?k=f
https://q3fk7/?b=o&n=r&i=b&q=b
```

### Generate with the Go CLI

The Go CLI (`barkus-gen`) uses the same FFI library and produces identical output for the same seed:

```bash
$ ./target/release/barkus-gen -grammar fixtures/grammars/json.ebnf -count 5 -seed 42
"d"
null
"fdxx"
true
true
```

```bash
$ ./target/release/barkus-gen -grammar fixtures/grammars/csv.ebnf -count 3 -seed 42
bk
"h","lh",g
gp,"d"
a,fip,lo
e,"iaj",jm
"bhp",a,o
```

### CLI flags

**Rust** (`barkus-cli`):

| Flag | Description | Default |
|------|-------------|---------|
| `<grammar>` | Path to grammar file (`.ebnf`, `.g4`, `.peg`) | required |
| `--count` | Number of samples | 10 |
| `--seed` | RNG seed (omit for random) | random |
| `--max-depth` | Max derivation depth | 20 |
| `--start` | Override start rule name | first rule |
| `--emit-tape` | Emit hex-encoded decision tapes to stderr | off |

**Go** (`barkus-gen`):

| Flag | Description | Default |
|------|-------------|---------|
| `-grammar` | Path to grammar file | required |
| `-count` | Number of samples | 10 |
| `-seed` | RNG seed (0 = random) | 0 |
| `-max-depth` | Max derivation depth (0 = default) | 0 |
| `-emit-tape` | Emit hex-encoded decision tapes to stderr | off |

## Using the Go library

```go
import "github.com/DataDog/barkus/go/pkg/barkus"

gen, err := barkus.NewGenerator(grammarSource, seed, maxDepth)
if err != nil {
    log.Fatal(err)
}
defer gen.Close()

buf := make([]byte, 64*1024)
out, err := gen.Generate(buf)
if err != nil {
    log.Fatal(err)
}
fmt.Println(string(out))
```

## SQL generation

`barkus-sql` generates random SQL queries that can reference real table and column names from your schema. It uses vendored ANTLR grammars with semantic hooks to produce syntactically valid, schema-aware output. Available dialects: **SQLite** (default), **PostgreSQL**, **Trino**, and **Generic** (ANSI).

For Go, [go-fuzz-headers](https://github.com/AdaLogics/go-fuzz-headers) provides a general-purpose `ConsumeSQLString()`, but it targets a single dialect with no schema awareness. Barkus gives you pluggable dialect grammars, custom schemas, and semantic hooks — so the generated SQL references your actual tables/columns and follows dialect-specific syntax.

See [`crates/barkus-sql/README.md`](crates/barkus-sql/README.md) for the full API reference and schema JSON format.

### Rust

```rust
use barkus_sql::SqlGenerator;
use rand::rngs::SmallRng;
use rand::SeedableRng;

let gen = SqlGenerator::new()?;                    // SQLite, synthetic schema
let mut rng = SmallRng::seed_from_u64(42);
let (sql, tape, _map) = gen.generate(&mut rng)?;
println!("{sql}");

// Replay the exact same query from the tape:
let (sql2, _) = gen.decode(&tape)?;
assert_eq!(sql, sql2);
```

Use the builder for other dialects or a custom schema:

```rust
use barkus_sql::{SqlGenerator, context::SqlContext, dialect::PostgresDialect};

let ctx: SqlContext = serde_json::from_str(schema_json)?;
let gen = SqlGenerator::builder()
    .context(ctx)
    .dialect(PostgresDialect)
    .grammar(lexer_g4, parser_g4)
    .build()?;
```

### Go

```go
gen, err := barkus.NewSQLGenerator(barkus.PostgreSQL,
    barkus.WithSchema(barkus.Schema{
        Tables: []barkus.Table{{
            Name: "accounts",
            Columns: []barkus.Column{
                {Name: "id", Type: barkus.SqlInteger},
                {Name: "email", Type: barkus.SqlText},
            },
        }},
    }),
    barkus.WithSeed(42),
)
if err != nil {
    log.Fatal(err)
}
defer gen.Close()

buf := make([]byte, 64*1024)
sql, err := gen.Generate(buf)
```

### Go fuzz test

```go
func FuzzPostgresSQL(f *testing.F) {
    gen, err := barkus.NewSQLGenerator(barkus.PostgreSQL, barkus.WithSeed(0))
    if err != nil {
        f.Fatal(err)
    }
    defer gen.Close()

    // Seed the corpus
    buf := make([]byte, 64*1024)
    for i := 0; i < 10; i++ {
        sql, err := gen.Generate(buf)
        if err == nil {
            f.Add(sql)
        }
    }

    f.Fuzz(func(t *testing.T, query []byte) {
        // Exercise your SQL parser, planner, or executor
        _ = query
    })
}
```

## Coverage visualization

`barkus-viz` generates coverage reports (text, HTML, or JSON) showing which grammar productions and alternatives your corpus exercises, plus hard-to-reach analysis and actionable recommendations to reduce failure rates:

```bash
# Text report to stdout
cargo run --release -p barkus-viz -- fixtures/grammars/json.ebnf -n 100000

# HTML report
cargo run --release -p barkus-viz -- fixtures/grammars/json.ebnf -n 100000 --format=html -o report.html
```

See [`crates/barkus-viz/README.md`](crates/barkus-viz/README.md) for all options.


## Development

```bash
make test        # Rust + Go tests
make test-go     # Go tests only
make ffi         # Build FFI library
make go-example  # Build Go CLI
make clean       # Clean all build artifacts
```
