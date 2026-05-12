package harness

import (
	"sync"
	"testing"

	barkus "github.com/DataDog/barkus/go/pkg/barkus"
	"github.com/minio/simdjson-go"
)

var (
	nearvalidOnce sync.Once
	nearvalidGen  *barkus.Generator
)

func nearvalidGenerator() *barkus.Generator {
	nearvalidOnce.Do(func() {
		g, err := barkus.NewGeneratorWithOptions(Grammar,
			barkus.WithFormat(GrammarFormat),
			barkus.WithValidityMode(barkus.NearValid),
			barkus.WithMaxDepth(24),
		)
		if err != nil {
			panic(err)
		}
		nearvalidGen = g
	})
	return nearvalidGen
}

func FuzzBarkusNearvalid(f *testing.F) {
	g := nearvalidGenerator()
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
