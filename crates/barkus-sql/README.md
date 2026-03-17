# barkus-sql

Structure-aware SQL fuzzer built on barkus. Generates random SQL queries that
reference real table/column names from your schema, with dialect-specific
formatting (PostgreSQL, SQLite, Trino).

## Rust — Library Usage

### Default (SQLite, synthetic schema)

```rust
use barkus_sql::SqlGenerator;
use rand::rngs::SmallRng;
use rand::SeedableRng;

let gen = SqlGenerator::new()?;
let mut rng = SmallRng::seed_from_u64(42);
let (sql, tape, _map) = gen.generate(&mut rng)?;
println!("{sql}");
```

### PostgreSQL with a Custom Schema

```rust
use barkus_sql::SqlGenerator;
use barkus_sql::context::SqlContext;
use barkus_sql::dialect::PostgresDialect;
use rand::rngs::SmallRng;
use rand::SeedableRng;

// Load schema from JSON (or build programmatically)
let ctx: SqlContext = serde_json::from_str(r#"{
    "tables": [
        {
            "name": "accounts",
            "columns": [
                {"name": "id",    "ty": "integer", "nullable": false},
                {"name": "email", "ty": "text",    "nullable": false},
                {"name": "score", "ty": "float",   "nullable": true}
            ]
        },
        {
            "name": "events",
            "columns": [
                {"name": "id",         "ty": "integer",   "nullable": false},
                {"name": "account_id", "ty": "integer",   "nullable": false},
                {"name": "kind",       "ty": "text",      "nullable": false},
                {"name": "fired_at",   "ty": "timestamp", "nullable": false}
            ]
        }
    ]
}"#)?;

let gen = SqlGenerator::builder()
    .context(ctx)
    .dialect(PostgresDialect)
    .grammar(
        include_str!("../../../grammars/antlr-sql/postgresql/PostgreSQLLexer.g4"),
        include_str!("../../../grammars/antlr-sql/postgresql/PostgreSQLParser.g4"),
    )
    .build()?;

let mut rng = SmallRng::seed_from_u64(0);
let (sql, tape, _) = gen.generate(&mut rng)?;
println!("{sql}");

// Replay the exact same query from the tape:
let (sql2, _) = gen.decode(&tape)?;
assert_eq!(sql, sql2);
```

### Schema JSON Format

```json
{
    "tables": [
        {
            "name": "my_table",
            "columns": [
                {"name": "id",   "ty": "integer",   "nullable": false},
                {"name": "name", "ty": "text",       "nullable": true}
            ]
        }
    ],
    "functions": [
        {"name": "COUNT", "args": ["integer"], "ret": "integer"}
    ]
}
```

Supported `ty` values: `integer`, `float`, `text`, `boolean`, `timestamp`,
`blob`, or `{"custom": "my_type"}`.

## Go — Using in a Fuzz Test

The Go bindings expose schema-aware SQL generation directly via the
`NewSQLGenerator` API. Grammars for each dialect are embedded in the FFI
library — no file paths needed.

### Prerequisites

Build the FFI library first:

```bash
cargo build --release -p barkus-ffi
```

### Default (SQLite, synthetic schema)

```go
package sqlfuzz_test

import (
    "testing"

    "github.com/DataDog/barkus/go/pkg/barkus"
)

func TestGenerateSQL(t *testing.T) {
    gen, err := barkus.NewSQLGenerator(barkus.SQLite, barkus.WithSeed(42))
    if err != nil {
        t.Fatal(err)
    }
    defer gen.Close()

    buf := make([]byte, 64*1024)
    sql, err := gen.Generate(buf)
    if err != nil {
        t.Fatal(err)
    }
    t.Logf("generated: %s", sql)
}
```

### PostgreSQL with a custom schema

```go
gen, err := barkus.NewSQLGenerator(barkus.PostgreSQL,
    barkus.WithSchema(barkus.Schema{
        Tables: []barkus.Table{
            {
                Name: "accounts",
                Columns: []barkus.Column{
                    {Name: "id",    Type: barkus.SqlInteger},
                    {Name: "email", Type: barkus.SqlText},
                    {Name: "score", Type: barkus.SqlFloat, Nullable: true},
                },
            },
        },
    }),
    barkus.WithSeed(0),
)
```

You can also pass a raw JSON schema string with `WithSchemaJSON(jsonStr)`.

### Decision tape round-trip

```go
buf := make([]byte, 64*1024)
tapeBuf := make([]byte, 64*1024)
sql, tape, err := gen.GenerateWithTape(buf, tapeBuf)
// ...

decodeBuf := make([]byte, 64*1024)
decoded, err := gen.Decode(tape, decodeBuf)
// decoded == sql
```

### Using in a Go fuzz target

```go
func FuzzPostgresSQL(f *testing.F) {
    gen, err := barkus.NewSQLGenerator(barkus.PostgreSQL, barkus.WithSeed(0))
    if err != nil {
        f.Fatal(err)
    }
    defer gen.Close()

    // Seed the corpus with generated SQL.
    buf := make([]byte, 64*1024)
    for i := 0; i < 10; i++ {
        sql, err := gen.Generate(buf)
        if err == nil {
            f.Add(sql)
        }
    }

    f.Fuzz(func(t *testing.T, query []byte) {
        // Exercise your SQL parser, planner, or executor:
        //   _, err := mydb.Prepare(string(query))
        _ = query
    })
}
```

## Decision Tapes

Every call to `generate()` returns a `DecisionTape` — a compact byte sequence
recording every random choice. Replaying a tape with `decode()` reproduces the
exact same output. This is useful for:

- **Reproducing crashes**: store the tape bytes alongside a failure report.
- **Corpus minimization**: tapes are smaller than the SQL they produce.
- **Deterministic CI**: commit tapes instead of generated strings.

## Available Dialects

| Dialect           | Rust type          | Grammar files                              |
|-------------------|--------------------|--------------------------------------------|
| SQLite (default)  | `SqliteDialect`    | `grammars/antlr-sql/sqlite/`               |
| PostgreSQL        | `PostgresDialect`  | `grammars/antlr-sql/postgresql/`           |
| Trino             | `TrinoDialect`     | `grammars/antlr-sql/trino/`               |
| Generic (ANSI)    | `GenericDialect`   | (use with any grammar)                     |
