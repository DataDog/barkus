# Generating non-valid inputs with Barkus

Barkus generates grammar-valid outputs by default. To produce **slightly invalid** or
**edge-case** inputs, you mutate the **decision tape** — a flat byte sequence where each
byte encodes exactly one structural decision (which alternative, how many repetitions, etc.).

Flipping a single tape byte changes one decision without scrambling the rest of the output.
This is the core mechanism for exploring the space of near-valid inputs.

## Workflow

```
1. Generate valid output + decision tape
2. Mutate tape bytes (fuzzer or manual)
3. Decode mutated tape → grammar-shaped output with different decisions
4. Feed to your system under test
```

## Examples

### Go: fuzz test with `go test -fuzz`

Seed corpus with decision tapes, then let the Go fuzzer mutate them.

- **[go-fuzz-json/](go-fuzz-json/)** — Fuzz a JSON parser. Generates JSON tapes as seeds,
  decodes mutated tapes, feeds output to `encoding/json`.
- **[go-fuzz-sql/](go-fuzz-sql/)** — Fuzz a SQL parser. Uses `SQLGenerator` with a custom
  PostgreSQL schema.

```bash
# Run the JSON fuzz test for 10 seconds
make ffi
go test ./examples/go-fuzz-json/ -fuzz=FuzzJSON -fuzztime=10s

# Run the SQL fuzz test for 10 seconds
go test ./examples/go-fuzz-sql/ -fuzz=FuzzSQL -fuzztime=10s
```

### Go: manual tape manipulation

Flip individual tape bytes and decode to see exactly what changes.

- **[tape-exploration/](tape-exploration/)** — Standalone program that generates a JSON
  output + tape, flips each body byte one at a time, and prints the resulting output.

```bash
make ffi
go run ./examples/tape-exploration/
```

### Rust: direct mutation operators

Use `barkus-core`'s mutation engine to apply targeted structural mutations.

- **[rust-tape-mutation/](rust-tape-mutation/)** — Demonstrates all six mutation operators
  (point mutate, range re-randomize, splice, subtree regenerate, toggle optional, perturb
  repetition) and shows how each one changes the output.

```bash
cargo run -p rust-tape-mutation
```

### CLI: quick tape round-trip

Generate a tape, hex-edit a byte, decode:

```bash
# Generate one JSON sample + tape (hex on stderr)
cargo run -p barkus-cli -- generate fixtures/grammars/json.ebnf --count 1 --seed 42 --emit-tape

# Decode a tape back to output
cargo run -p barkus-cli -- decode fixtures/grammars/json.ebnf --tape <hex-string>
```

## How tape mutation produces non-valid inputs

Each tape byte maps to one structural decision:

| Byte controls | Mutation effect |
|---------------|----------------|
| Alternative choice (`byte % n`) | Switches to a different grammar branch |
| Repetition count | Adds or removes repeated elements |
| Optional toggle | Flips present/absent |
| Character class | Picks a different character |

A single byte flip is a **surgical change** — the surrounding output stays identical.
This means the fuzzer explores variations that are structurally close to valid inputs,
which is exactly where parsers tend to have bugs.
