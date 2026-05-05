// Package sqlfuzz is an example of tape-based SQL fuzzing with barkus.
//
// The pattern is not test-only: the same SQLGenerator API can be used
// outside *_test.go to produce SQL payloads at runtime. The fuzz harness
// here just lets `go test -fuzz` mutate the seed tapes for you.
package sqlfuzz

import (
	"testing"

	"github.com/DataDog/barkus/go/pkg/barkus"
)

// schema describes the tables and columns the generator may reference.
//
// Without it, the generator has no real identifiers to plug into FROM /
// SELECT / WHERE / JOIN, so generated SQL would either fail validation or
// look generic. Pass your own schema to fuzz a parser/planner against the
// shapes it actually sees in production.
var schema = barkus.Schema{
	Tables: []barkus.Table{
		{
			Name: "users",
			Columns: []barkus.Column{
				{Name: "id", Type: barkus.SqlInteger},
				{Name: "email", Type: barkus.SqlText},
				{Name: "active", Type: barkus.SqlBoolean, Nullable: true},
			},
		},
		{
			Name: "orders",
			Columns: []barkus.Column{
				{Name: "id", Type: barkus.SqlInteger},
				{Name: "user_id", Type: barkus.SqlInteger},
				{Name: "total", Type: barkus.SqlFloat},
				{Name: "created_at", Type: barkus.SqlTimestamp},
			},
		},
	},
}

// newGen builds a SQLGenerator. NewSQLGenerator takes the dialect plus
// functional options:
//
//	WithSchema / WithSchemaJSON — provide the table/column metadata.
//	WithSeed                    — RNG seed; 0 means non-deterministic.
//	WithMaxDepth                — derivation depth cap (default 20).
//	WithMaxTotalNodes           — hard cap on AST size.
//	WithValidityMode            — Strict / NearValid / Havoc.
func newGen(t testing.TB, seed uint64) *barkus.SQLGenerator {
	t.Helper()
	gen, err := barkus.NewSQLGenerator(barkus.PostgreSQL,
		barkus.WithSchema(schema),
		barkus.WithSeed(seed),
	)
	if err != nil {
		t.Fatal(err)
	}
	return gen
}

func TestSeedCorpus(t *testing.T) {
	// 64 KiB is generous for most SQL outputs and tapes. Size to fit your
	// largest expected output; the call returns an error on overflow.
	buf := make([]byte, 64*1024)
	tapeBuf := make([]byte, 64*1024)
	decodeBuf := make([]byte, 64*1024)

	// SQL grammars are deeply recursive; some seeds exhaust the depth budget.
	// Try a range until one succeeds.
	for seed := uint64(1); seed < 20; seed++ {
		gen := newGen(t, seed)
		out, tape, err := gen.GenerateWithTape(buf, tapeBuf)
		if err != nil {
			gen.Close()
			continue
		}
		t.Logf("seed=%d output: %s", seed, out)
		t.Logf("seed=%d tape:   %d bytes", seed, len(tape))

		decoded, err := gen.Decode(tape, decodeBuf)
		gen.Close()
		if err != nil {
			t.Fatalf("seed=%d decode error: %v", seed, err)
		}
		t.Logf("seed=%d decoded: %s", seed, decoded)
		return
	}
	t.Fatal("no seed produced a successful generation")
}

func FuzzSQL(f *testing.F) {
	buf := make([]byte, 64*1024)
	tapeBuf := make([]byte, 64*1024)

	// SQL grammars are deeply recursive; some seeds exhaust the depth budget.
	for seed := uint64(1); seed < 20; seed++ {
		gen := newGen(f, seed)
		_, tape, err := gen.GenerateWithTape(buf, tapeBuf)
		gen.Close()
		if err != nil {
			continue
		}
		// tapeBuf is reused across iterations, so copy out the bytes for f.Add.
		tapeCopy := make([]byte, len(tape))
		copy(tapeCopy, tape)
		f.Add(tapeCopy)
	}

	f.Fuzz(func(t *testing.T, tape []byte) {
		if len(tape) < 3 {
			return
		}

		gen := newGen(t, 0)
		defer gen.Close()

		decodeBuf := make([]byte, 64*1024)
		out, err := gen.Decode(tape, decodeBuf)
		if err != nil {
			return // invalid tape — fuzzer will learn to avoid
		}

		// Feed `out` to your SQL parser or system under test here.
		_ = out
	})
}
