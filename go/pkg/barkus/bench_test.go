package barkus

import (
	"bytes"
	"testing"
)

const benchGrammar = `
start = list ;
list = "[" items "]" ;
items = "" | item | item "," items ;
item = "a" | "b" | "c" | "1" | "2" | "3" ;
`

// Compares the stateless wrapper (compile + decode + destroy on every
// call — the path used by the original M3 simdjson-go harness) to the
// cached *Generator.Decode method (post-8287f5c). Same input both ways
// so the only variable is the FFI hot-path shape.
//
// Run with: go test -bench=. -benchmem ./go/pkg/barkus/

func BenchmarkDecodeStateless(b *testing.B) {
	tape := bytes.Repeat([]byte{0x42}, 64)
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_, _ = DecodeWithOptions(benchGrammar, tape,
			WithMaxDepth(8))
	}
}

func BenchmarkDecodeCached(b *testing.B) {
	g, err := NewGeneratorWithOptions(benchGrammar, WithMaxDepth(8))
	if err != nil {
		b.Fatalf("compile: %v", err)
	}
	defer g.Close()
	tape := bytes.Repeat([]byte{0x42}, 64)
	buf := make([]byte, 1024)
	b.ResetTimer()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		_, _ = g.Decode(tape, buf)
	}
}

// Concurrent throughput: GOMAXPROCS goroutines hammering one shared
// Generator. Wait-free under the new const-handle FFI; would have been
// a data race against any earlier *mut Handle signature.
func BenchmarkDecodeCachedParallel(b *testing.B) {
	g, err := NewGeneratorWithOptions(benchGrammar, WithMaxDepth(8))
	if err != nil {
		b.Fatalf("compile: %v", err)
	}
	defer g.Close()
	tape := bytes.Repeat([]byte{0x42}, 64)
	b.ResetTimer()
	b.ReportAllocs()
	b.RunParallel(func(pb *testing.PB) {
		buf := make([]byte, 1024)
		for pb.Next() {
			_, _ = g.Decode(tape, buf)
		}
	})
}
