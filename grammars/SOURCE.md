# Vendored ANTLR Grammars

All grammars are vendored from [grammars-v4](https://github.com/antlr/grammars-v4)
at a single pinned commit so every Barkus build is reproducible against the
same upstream sources.

**Pinned commit**: `da37ccd31749ab04bceb45e800207bb6fede2a76`

## Vendored grammars

| Local path                      | Upstream path                    | License    | Notes |
|---------------------------------|----------------------------------|------------|-------|
| `json/JSON.g4`                  | `json/JSON.g4`                   | BSD-3      | Combined grammar |
| `sqlite/SQLiteLexer.g4`         | `sql/sqlite/SQLiteLexer.g4`      | MIT        | |
| `sqlite/SQLiteParser.g4`        | `sql/sqlite/SQLiteParser.g4`     | MIT        | |
| `postgresql/PostgreSQLLexer.g4` | `sql/postgresql/PostgreSQLLexer.g4` | MIT     | |
| `postgresql/PostgreSQLParser.g4`| `sql/postgresql/PostgreSQLParser.g4`| MIT     | |
| `trino/TrinoLexer.g4`           | `sql/trino/TrinoLexer.g4`        | Apache 2.0 | |
| `trino/TrinoParser.g4`          | `sql/trino/TrinoParser.g4`       | Apache 2.0 | |
| `mysql/MySQLLexer.g4`           | `sql/mysql/Oracle/MySQLLexer.g4` | BSD-3      | Java semantic predicates stripped at fetch (Oracle variant) |
| `mysql/MySQLParser.g4`          | `sql/mysql/Oracle/MySQLParser.g4`| BSD-3      | Java semantic predicates stripped at fetch |

Per-grammar `LICENSE` files are kept alongside the grammars when upstream
ships one (sqlite/, postgresql/, trino/). MySQL and JSON licenses are
embedded as headers inside the `.g4` files themselves.

## Updating

Run `./update.sh` from this directory. To bump the pin, edit `COMMIT` in
`update.sh` and re-run — then verify `cargo test -p barkus-antlr -p barkus-sql`
and the simdjson-go smoke still pass.
