package harness

import (
	"os"

	barkus "github.com/DataDog/barkus/go/pkg/barkus"
)

// Grammar is the JSON grammar source used by every barkus_* harness,
// loaded from antlr/grammars-v4's JSON.g4 (vendored at grammars/json/
// and baked into barkus-base:dev by Dockerfile.base). The combined ANTLR
// grammar covers escape sequences, scientific notation, and Unicode that
// the previous hand-written EBNF placeholder skipped.
//
// Path resolution: BARKUS_JSON_GRAMMAR env var > /opt/barkus/share/JSON.g4.
var Grammar string

// GrammarFormat tells the FFI which parser to use for Grammar.
const GrammarFormat = barkus.GrammarAntlr

func init() {
	path := os.Getenv("BARKUS_JSON_GRAMMAR")
	if path == "" {
		path = "/opt/barkus/share/JSON.g4"
	}
	data, err := os.ReadFile(path)
	if err != nil {
		panic("simdjson harness: cannot read JSON grammar at " + path + ": " + err.Error())
	}
	Grammar = string(data)
}
