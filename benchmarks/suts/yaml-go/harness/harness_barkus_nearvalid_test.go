package harness

import (
	"sync"
	"testing"

	barkus "github.com/DataDog/barkus/go/pkg/barkus"
	yaml "gopkg.in/yaml.v3"
)

var (
	nearvalidOnce sync.Once
	nearvalidGen  *barkus.Generator
)

func nearvalidGenerator() *barkus.Generator {
	nearvalidOnce.Do(func() {
		g, err := barkus.NewGeneratorWithOptions(Grammar,
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
		var v any
		_ = yaml.Unmarshal(out, &v)
	})
}
