#!/usr/bin/env bash
# Fetch ANTLR SQL grammars from grammars-v4 at a pinned commit.
# Run from the grammars/antlr-sql/ directory.
set -euo pipefail

COMMIT="7bb150f62f54e587f18e323b71fa0309bdec5056"
BASE="https://raw.githubusercontent.com/antlr/grammars-v4/${COMMIT}"

fetch() {
    local dir="$1"
    local remote_dir="$2"
    local file="$3"
    echo "  ${dir}/${file}"
    curl -sSfL "${BASE}/${remote_dir}/${file}" -o "${dir}/${file}"
}

echo "Fetching grammars at commit ${COMMIT:0:12}..."

# SQLite
fetch sqlite "sql/sqlite" "SQLiteLexer.g4"
fetch sqlite "sql/sqlite" "SQLiteParser.g4"

# PostgreSQL
fetch postgresql "sql/postgresql" "PostgreSQLLexer.g4"
fetch postgresql "sql/postgresql" "PostgreSQLParser.g4"

# Trino
fetch trino "sql/trino" "TrinoLexer.g4"
fetch trino "sql/trino" "TrinoParser.g4"

echo "Done."
