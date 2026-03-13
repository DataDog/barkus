# ANTLR SQL Grammar Sources

Grammars are vendored from the [grammars-v4](https://github.com/antlr/grammars-v4) repository.

**Pinned commit**: `7bb150f62f54e587f18e323b71fa0309bdec5056`

**Upstream repository**: https://github.com/antlr/grammars-v4

## Vendored grammars

| Dialect    | Lexer                         | Parser                         | License |
|------------|-------------------------------|--------------------------------|---------|
| PostgreSQL | `PostgreSQLLexer.g4`          | `PostgreSQLParser.g4`          | MIT        |
| SQLite     | `SQLiteLexer.g4`              | `SQLiteParser.g4`              | MIT        |
| Trino      | `TrinoLexer.g4`               | `TrinoParser.g4`               | Apache 2.0 |

## Updating

Run `./update.sh` from this directory to re-fetch grammars at the pinned commit.
To update to a newer commit, edit the `COMMIT` variable in `update.sh` and re-run.
