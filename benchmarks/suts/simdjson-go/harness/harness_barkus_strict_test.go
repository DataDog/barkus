package harness

import (
	"sync"
	"testing"

	barkus "github.com/DataDog/barkus/go/pkg/barkus"
	"github.com/minio/simdjson-go"
)

// One Generator per ValidityMode, shared across all goroutines. The FFI's
// barkus_decode takes a const handle and touches no mutable state, so
// many testing.F workers can call g.Decode in parallel without
// synchronization.
var (
	strictOnce sync.Once
	strictGen  *barkus.Generator
)

func strictGenerator() *barkus.Generator {
	strictOnce.Do(func() {
		g, err := barkus.NewGeneratorWithOptions(Grammar,
			barkus.WithFormat(GrammarFormat),
			barkus.WithValidityMode(barkus.Strict),
			barkus.WithMaxDepth(24),
		)
		if err != nil {
			panic(err)
		}
		strictGen = g
	})
	return strictGen
}

func FuzzBarkusStrict(f *testing.F) {
	g := strictGenerator()
	f.Fuzz(func(t *testing.T, data []byte) {
		if len(data) == 0 {
			return
		}
		buf := make([]byte, 64*1024)
		out, err := g.Decode(data, buf)
		if err != nil || len(out) == 0 {
			return
		}
		_, _ = simdjson.Parse(out, nil)
	})
}
