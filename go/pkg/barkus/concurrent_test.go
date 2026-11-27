package barkus

import (
	"bytes"
	"strings"
	"sync"
	"testing"
)

const concurrentTestGrammar = `
start = list ;
list = "[" items "]" ;
items = "" | item | item "," items ;
item = "a" | "b" | "c" | "1" | "2" | "3" ;
`

// TestGeneratorDecodeConcurrent exercises the Sync contract on Handle.
// Decode reads only the immutable ir + profile fields and never touches
// the rng mutex; this test would deadlock or race if that contract
// breaks. Run with -race in CI.
func TestGeneratorDecodeConcurrent(t *testing.T) {
	g, err := NewGeneratorWithOptions(concurrentTestGrammar, WithMaxDepth(8))
	if err != nil {
		t.Fatalf("compile: %v", err)
	}
	defer g.Close()

	const goroutines = 16
	const iters = 500
	var wg sync.WaitGroup
	wg.Add(goroutines)
	for w := 0; w < goroutines; w++ {
		go func(seed byte) {
			defer wg.Done()
			tape := bytes.Repeat([]byte{seed}, 64)
			buf := make([]byte, 1024)
			for i := 0; i < iters; i++ {
				if _, err := g.Decode(tape, buf); err != nil {
					return // grammar may reject some tapes; that's fine
				}
			}
		}(byte(w))
	}
	wg.Wait()
}

// TestGeneratorGenerateConcurrent exercises the rng-mutex path. Generate
// serializes on the Mutex<SmallRng> inside Handle so concurrent callers
// produce correct (if not parallel) output rather than racing.
func TestGeneratorGenerateConcurrent(t *testing.T) {
	g, err := NewGeneratorWithOptions(concurrentTestGrammar, WithMaxDepth(8))
	if err != nil {
		t.Fatalf("compile: %v", err)
	}
	defer g.Close()

	const goroutines = 8
	const iters = 200
	var wg sync.WaitGroup
	wg.Add(goroutines)
	for w := 0; w < goroutines; w++ {
		go func() {
			defer wg.Done()
			buf := make([]byte, 1024)
			for i := 0; i < iters; i++ {
				_, _ = g.Generate(buf)
			}
		}()
	}
	wg.Wait()
}

// TestConcurrentLastError verifies that thread-local error messages are never
// lost ("barkus: unknown error") or cross-contaminated across goroutines when
// parallel FFI calls fail simultaneously.
func TestConcurrentLastError(t *testing.T) {
	g, err := NewGenerator(`start = "hello world" ;`, 1, 0)
	if err != nil {
		t.Fatalf("NewGenerator: %v", err)
	}
	defer g.Close()

	const goroutines = 16
	const iters = 200
	var wg sync.WaitGroup
	wg.Add(goroutines)
	for w := 0; w < goroutines; w++ {
		go func(id int) {
			defer wg.Done()
			tinyBuf := make([]byte, 1)
			for i := 0; i < iters; i++ {
				var callErr error
				var wantSubstr string
				switch (id + i) % 3 {
				case 0:
					_, callErr = NewGenerator("not valid ebnf ???", 1, 0)
					wantSubstr = "compile error"
				case 1:
					_, callErr = g.Generate(tinyBuf)
					wantSubstr = "output buffer too small"
				case 2:
					_, callErr = NewSQLGenerator(Dialect("nosuchdialect"), WithSeed(1))
					wantSubstr = "unknown dialect"
				}
				if callErr == nil {
					t.Errorf("worker %d iter %d: expected error containing %q, got nil", id, i, wantSubstr)
					return
				}
				msg := callErr.Error()
				if msg == "barkus: unknown error" || !strings.Contains(msg, wantSubstr) {
					t.Errorf("worker %d iter %d: got %q, want substring %q", id, i, msg, wantSubstr)
					return
				}
			}
		}(w)
	}
	wg.Wait()
}
