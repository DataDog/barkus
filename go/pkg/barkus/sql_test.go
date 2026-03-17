package barkus

import (
	"strings"
	"testing"
)

// sqlGenerate tries multiple seeds and returns the first successful output.
// SQL grammars are deeply recursive and some seeds exhaust the depth budget;
// this mirrors the pattern used in the Rust test suite.
func sqlGenerate(t *testing.T, dialect Dialect, opts ...SQLOption) (gen *SQLGenerator, out []byte) {
	t.Helper()
	buf := make([]byte, 64*1024)
	for seed := uint64(0); seed < 20; seed++ {
		allOpts := append([]SQLOption{WithSeed(seed)}, opts...)
		g, err := NewSQLGenerator(dialect, allOpts...)
		if err != nil {
			t.Fatalf("NewSQLGenerator(seed=%d): %v", seed, err)
		}
		result, err := g.Generate(buf)
		if err != nil {
			g.Close()
			continue
		}
		if len(result) > 0 {
			return g, result
		}
		g.Close()
	}
	t.Fatal("no seeds produced successful generation across 20 attempts")
	return nil, nil
}

// sqlGenerateWithTape tries multiple seeds and returns the first successful tape capture.
func sqlGenerateWithTape(t *testing.T, dialect Dialect, opts ...SQLOption) (gen *SQLGenerator, out, tape []byte) {
	t.Helper()
	buf := make([]byte, 64*1024)
	tapeBuf := make([]byte, 64*1024)
	for seed := uint64(0); seed < 20; seed++ {
		allOpts := append([]SQLOption{WithSeed(seed)}, opts...)
		g, err := NewSQLGenerator(dialect, allOpts...)
		if err != nil {
			t.Fatalf("NewSQLGenerator(seed=%d): %v", seed, err)
		}
		o, tp, err := g.GenerateWithTape(buf, tapeBuf)
		if err != nil {
			g.Close()
			continue
		}
		if len(o) > 0 {
			return g, o, tp
		}
		g.Close()
	}
	t.Fatal("no seeds produced successful tape generation across 20 attempts")
	return nil, nil, nil
}

func TestSQLDefaultGenerator(t *testing.T) {
	gen, out := sqlGenerate(t, SQLite)
	defer gen.Close()
	if len(out) == 0 {
		t.Error("expected non-empty output")
	}
}

func TestSQLPostgresDialect(t *testing.T) {
	gen, out := sqlGenerate(t, PostgreSQL)
	defer gen.Close()
	if len(out) == 0 {
		t.Error("expected non-empty output")
	}
}

func TestSQLCustomSchema(t *testing.T) {
	schema := Schema{
		Tables: []Table{
			{
				Name: "accounts",
				Columns: []Column{
					{Name: "id", Type: SqlInteger, Nullable: false},
					{Name: "email", Type: SqlText, Nullable: false},
					{Name: "score", Type: SqlFloat, Nullable: true},
				},
			},
		},
	}

	gen, out := sqlGenerate(t, SQLite, WithSchema(schema))
	defer gen.Close()
	if len(out) == 0 {
		t.Error("expected non-empty output")
	}
}

func TestSQLSchemaJSON(t *testing.T) {
	jsonSchema := `{
		"tables": [
			{
				"name": "items",
				"columns": [
					{"name": "id", "ty": "integer", "nullable": false},
					{"name": "label", "ty": "text", "nullable": true}
				]
			}
		]
	}`

	gen, out := sqlGenerate(t, SQLite, WithSchemaJSON(jsonSchema))
	defer gen.Close()
	if len(out) == 0 {
		t.Error("expected non-empty output")
	}
}

func TestSQLDeterministicSeed(t *testing.T) {
	// Find a seed that works, then verify determinism with two generators.
	// Start at 1 because seed 0 means "random" in the FFI.
	buf := make([]byte, 64*1024)
	for seed := uint64(1); seed < 20; seed++ {
		g1, err := NewSQLGenerator(SQLite, WithSeed(seed))
		if err != nil {
			t.Fatalf("NewSQLGenerator g1: %v", err)
		}
		g2, err := NewSQLGenerator(SQLite, WithSeed(seed))
		if err != nil {
			g1.Close()
			t.Fatalf("NewSQLGenerator g2: %v", err)
		}

		out1, err1 := g1.Generate(buf)
		buf2 := make([]byte, 64*1024)
		out2, err2 := g2.Generate(buf2)
		g1.Close()
		g2.Close()

		if err1 != nil && err2 != nil {
			continue // both failed — consistent, try next seed
		}
		if (err1 == nil) != (err2 == nil) {
			t.Fatalf("seed %d: inconsistent results: err1=%v, err2=%v", seed, err1, err2)
		}
		if string(out1) != string(out2) {
			t.Fatalf("seed %d: mismatch: %q != %q", seed, string(out1), string(out2))
		}
		return // success
	}
	t.Fatal("no seeds produced successful generation for determinism test")
}

func TestSQLGenerateWithTape(t *testing.T) {
	gen, out, tape := sqlGenerateWithTape(t, SQLite)
	defer gen.Close()
	if len(out) == 0 {
		t.Error("expected non-empty output")
	}
	if len(tape) == 0 {
		t.Error("expected non-empty tape")
	}
}

func TestSQLDecodeRoundtrip(t *testing.T) {
	gen, out, tape := sqlGenerateWithTape(t, SQLite)
	defer gen.Close()

	decodeBuf := make([]byte, 64*1024)
	decoded, err := gen.Decode(tape, decodeBuf)
	if err != nil {
		t.Fatalf("Decode: %v", err)
	}
	if string(out) != string(decoded) {
		t.Errorf("roundtrip mismatch: generated %q, decoded %q", string(out), string(decoded))
	}
}

func TestSQLClosedGenerator(t *testing.T) {
	gen, err := NewSQLGenerator(SQLite, WithSeed(1))
	if err != nil {
		t.Fatalf("NewSQLGenerator: %v", err)
	}
	gen.Close()

	buf := make([]byte, 1024)
	_, err = gen.Generate(buf)
	if err == nil {
		t.Fatal("expected error on closed generator")
	}
}

func TestSQLInvalidDialect(t *testing.T) {
	_, err := NewSQLGenerator(Dialect("nosuchdialect"), WithSeed(1))
	if err == nil {
		t.Fatal("expected error for invalid dialect")
	}
	if !strings.Contains(err.Error(), "unknown dialect") {
		t.Errorf("expected 'unknown dialect' error, got: %v", err)
	}
}
