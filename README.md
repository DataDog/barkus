# Barkus

Structure-aware fuzzer that generates samples from grammars (EBNF, ANTLR v4, PEG). Write a grammar, get random valid outputs — useful for fuzz testing parsers, protocols, and file formats.

## Quick start

### Build

```bash
# Rust CLI
cargo build -p barkus-cli --release

# Go CLI (builds the FFI library first)
make go-example
```

### Write a grammar

Barkus uses a standard EBNF syntax. Here's a simple JSON grammar (`fixtures/grammars/json.ebnf`):

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
$ cargo run -p barkus-cli -- fixtures/grammars/sql-select.ebnf --count 3 --seed 42
SELECT g, k, p9, l4, m_ FROM p_e ORDER BY ab_d;
SELECT o_e FROM o_j;
SELECT l_, i, db0 FROM o ORDER BY b1;
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

## Coverage visualization

`barkus-viz` generates coverage reports (text, HTML, or JSON) showing which grammar productions and alternatives your corpus exercises, plus hard-to-reach analysis and actionable recommendations to reduce failure rates:

```bash
# Text report to stdout
cargo run --release -p barkus-viz -- fixtures/grammars/json.ebnf -n 100000

# HTML report
cargo run --release -p barkus-viz -- fixtures/grammars/json.ebnf -n 100000 --format=html -o report.html
```

See [`crates/barkus-viz/README.md`](crates/barkus-viz/README.md) for all options.

## Fixture grammars

Sample grammars live in `fixtures/grammars/`:

| Grammar | Generates |
|---------|-----------|
| `json.ebnf` / `json.g4` | JSON values |
| `csv.ebnf` | CSV rows |
| `arithmetic.ebnf` / `arithmetic.peg` | Math expressions |
| `url.ebnf` | URLs with query strings |
| `sql-select.ebnf` | SQL SELECT statements |
| `ottl.ebnf` / `ottl.peg` | OpenTelemetry Transformation Language |

## Development

```bash
make test        # Rust + Go tests
make test-go     # Go tests only
make ffi         # Build FFI library
make go-example  # Build Go CLI
make clean       # Clean all build artifacts
```
