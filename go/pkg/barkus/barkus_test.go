package barkus

import (
	"strings"
	"testing"
)

func TestRoundtripHello(t *testing.T) {
	g, err := NewGenerator(`start = "hello" ;`, 42, 0)
	if err != nil {
		t.Fatalf("NewGenerator: %v", err)
	}
	defer g.Close()

	buf := make([]byte, 1024)
	out, err := g.Generate(buf)
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}
	if string(out) != "hello" {
		t.Errorf("expected %q, got %q", "hello", string(out))
	}
}

func TestMultipleGenerates(t *testing.T) {
	g, err := NewGenerator(`start = "a" | "b" | "c" ;`, 99, 0)
	if err != nil {
		t.Fatalf("NewGenerator: %v", err)
	}
	defer g.Close()

	buf := make([]byte, 64)
	for i := 0; i < 10; i++ {
		out, err := g.Generate(buf)
		if err != nil {
			t.Fatalf("Generate #%d: %v", i, err)
		}
		s := string(out)
		if s != "a" && s != "b" && s != "c" {
			t.Errorf("Generate #%d: unexpected output %q", i, s)
		}
	}
}

func TestInvalidGrammar(t *testing.T) {
	_, err := NewGenerator("not valid ebnf ???", 1, 0)
	if err == nil {
		t.Fatal("expected error for invalid grammar")
	}
	if !strings.Contains(err.Error(), "compile error") {
		t.Errorf("expected compile error, got: %v", err)
	}
}

func TestDeterministicSeed(t *testing.T) {
	grammar := `start = "x" | "y" | "z" ;`

	g1, err := NewGenerator(grammar, 42, 0)
	if err != nil {
		t.Fatalf("NewGenerator g1: %v", err)
	}
	defer g1.Close()

	g2, err := NewGenerator(grammar, 42, 0)
	if err != nil {
		t.Fatalf("NewGenerator g2: %v", err)
	}
	defer g2.Close()

	buf1 := make([]byte, 64)
	buf2 := make([]byte, 64)
	for i := 0; i < 10; i++ {
		out1, err := g1.Generate(buf1)
		if err != nil {
			t.Fatalf("g1.Generate #%d: %v", i, err)
		}
		out2, err := g2.Generate(buf2)
		if err != nil {
			t.Fatalf("g2.Generate #%d: %v", i, err)
		}
		if string(out1) != string(out2) {
			t.Errorf("seed mismatch #%d: %q != %q", i, string(out1), string(out2))
		}
	}
}

func TestClosedGenerator(t *testing.T) {
	g, err := NewGenerator(`start = "x" ;`, 1, 0)
	if err != nil {
		t.Fatalf("NewGenerator: %v", err)
	}
	g.Close()

	buf := make([]byte, 64)
	_, err = g.Generate(buf)
	if err == nil {
		t.Fatal("expected error on closed generator")
	}
}
