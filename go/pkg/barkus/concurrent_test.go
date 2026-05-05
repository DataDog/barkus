package barkus

import (
	"bytes"
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
