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

The same `Generator` / `Decode` API is used inside fuzz tests and outside of them.
You can call it from a one-shot CLI, a load generator, or a replay tool — the fuzz
harnesses below just let `go test -fuzz` / `cargo fuzz` mutate the seed tapes for you.

## Examples

### Go: `go test -fuzz`

Seed corpus with decision tapes, then let the Go fuzzer mutate them.

- **[go-fuzz-json/](go-fuzz-json/)** — Fuzz `encoding/json` by decoding mutated tapes
  into JSON-shaped strings.
- **[go-fuzz-sql/](go-fuzz-sql/)** — Fuzz a SQL parser. Uses `SQLGenerator` with a custom
  PostgreSQL schema.

```bash
make ffi
go test ./examples/go-fuzz-json/ -fuzz=FuzzJSON -fuzztime=10s
go test ./examples/go-fuzz-sql/  -fuzz=FuzzSQL  -fuzztime=10s
```

### Rust: `cargo fuzz`

Same pattern as the Go example, using libFuzzer via [`cargo-fuzz`](https://rust-fuzz.github.io/book/cargo-fuzz.html).

- **[rust-fuzz-json/](rust-fuzz-json/)** — Decodes mutated tapes and feeds the output to
  `serde_json`.

```bash
# One-time install
cargo install cargo-fuzz

cargo +nightly fuzz run fuzz_json --fuzz-dir examples/rust-fuzz-json
```

Optionally seed the corpus with valid tapes for faster coverage. The CLI emits
hex tapes on stderr; convert each to a binary file in the corpus directory:

```bash
mkdir -p examples/rust-fuzz-json/corpus/fuzz_json
cargo run -q -p barkus-cli -- \
  generate fixtures/grammars/json.ebnf --count 32 --seed 1 --emit-tape 2>&1 1>/dev/null \
  | awk '{print > ("examples/rust-fuzz-json/corpus/fuzz_json/seed-" NR ".hex")}'
for f in examples/rust-fuzz-json/corpus/fuzz_json/seed-*.hex; do
  xxd -r -p "$f" > "${f%.hex}" && rm "$f"
done
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
