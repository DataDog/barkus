// Package jsonfuzz is an example of tape-based fuzzing with barkus.
package jsonfuzz

import (
	"encoding/json"
	"os"
	"testing"

	"github.com/DataDog/barkus/go/pkg/barkus"
)

var jsonGrammar string

func TestMain(m *testing.M) {
	raw, err := os.ReadFile("../../fixtures/grammars/json.ebnf")
	if err != nil {
		panic("failed to read json.ebnf: " + err.Error())
	}
	jsonGrammar = string(raw)
	os.Exit(m.Run())
}

func TestSeedCorpus(t *testing.T) {
	gen, err := barkus.NewGenerator(jsonGrammar, 42, 0)
	if err != nil {
		t.Fatal(err)
	}
	defer gen.Close()

	buf := make([]byte, 64*1024)
	tapeBuf := make([]byte, 64*1024)

	out, tape, err := gen.GenerateWithTape(buf, tapeBuf)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("output:  %s", out)
	t.Logf("tape:    %d bytes", len(tape))

	decoded, err := barkus.Decode(jsonGrammar, tape, 0)
	if err != nil {
		t.Fatal(err)
	}
	if string(out) != string(decoded) {
		t.Fatalf("roundtrip mismatch: %q != %q", out, decoded)
	}
}

func FuzzJSON(f *testing.F) {
	f.Fuzz(func(t *testing.T, tape []byte) {
		if len(tape) < 3 {
			return
		}

		out, err := barkus.Decode(jsonGrammar, tape, 0)
		if err != nil {
			return // invalid tape — the fuzzer will learn to avoid these
		}
		// Feed the output to your parser or system under test.
		var v any
		if err := json.Unmarshal(out, &v); err != nil {
			return
		}
	})
}
