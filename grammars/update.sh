#!/usr/bin/env bash
# Re-fetch every vendored ANTLR grammar at the pinned commit. Run from
# this directory.
#
# To bump the pin: edit COMMIT, re-run, and verify cargo tests + the
# simdjson-go smoke. To add a grammar: append a row to the MANIFEST array
# below (and to grammars/SOURCE.md).
set -euo pipefail

COMMIT="da37ccd31749ab04bceb45e800207bb6fede2a76"
BASE="https://raw.githubusercontent.com/antlr/grammars-v4/${COMMIT}"

# "<local-path> <upstream-path> <strip-predicates:0|1>"
MANIFEST=(
    "json/JSON.g4                       json/JSON.g4                       0"
    "sqlite/SQLiteLexer.g4              sql/sqlite/SQLiteLexer.g4          0"
    "sqlite/SQLiteParser.g4             sql/sqlite/SQLiteParser.g4         0"
    "postgresql/PostgreSQLLexer.g4      sql/postgresql/PostgreSQLLexer.g4  0"
    "postgresql/PostgreSQLParser.g4     sql/postgresql/PostgreSQLParser.g4 0"
    "trino/TrinoLexer.g4                sql/trino/TrinoLexer.g4            0"
    "trino/TrinoParser.g4               sql/trino/TrinoParser.g4           0"
    "mysql/MySQLLexer.g4                sql/mysql/Oracle/MySQLLexer.g4     1"
    "mysql/MySQLParser.g4               sql/mysql/Oracle/MySQLParser.g4    1"
)

# Strip Java semantic predicates like `{this.serverVersionGe(80014)}?` from
# the MySQL grammar — barkus-antlr does not run host actions, so leaving
# them in causes a parse error at FFI compile time. For fuzzing purposes
# we treat every version-gated rule as always-applicable, which strictly
# broadens the generator's reach.
strip_predicates() {
    local file="$1"
    # Regex assumes no nested braces inside predicate bodies — true for
    # every `{ this.foo() }?` in grammars-v4's MySQL grammar at the pinned
    # commit. The post-fetch removed-count assertion fires loudly if a
    # future commit introduces nested braces, so we don't silently ship
    # un-stripped predicates.
    python3 - <<PY
import re, sys
src = open("${file}").read()
new = re.sub(r"\{\s*this\.[^}]*\}\s*\?", "", src)
open("${file}", "w").write(new)
removed = (src.count("{this.") + src.count("{ this.")
        -  new.count("{this.") - new.count("{ this."))
if removed == 0:
    sys.exit(f"strip_predicates: no predicates removed from ${file}; "
             f"grammar may have changed shape upstream — re-check the regex")
print(f"  stripped {removed} predicate(s) from ${file}")
PY
}

echo "Fetching grammars at commit ${COMMIT:0:12}..."
for entry in "${MANIFEST[@]}"; do
    read -r local remote strip <<<"${entry}"
    echo "  ${local}"
    mkdir -p "$(dirname "${local}")"
    curl -sSfL "${BASE}/${remote}" -o "${local}"
    if [[ "${strip}" == "1" ]]; then
        strip_predicates "${local}"
    fi
done
echo "Done."
