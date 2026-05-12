package harness

import (
	"sync"
	"testing"

	barkus "github.com/DataDog/barkus/go/pkg/barkus"
	yaml "gopkg.in/yaml.v3"
)

var (
	strictOnce sync.Once
	strictGen  *barkus.Generator
)

func strictGenerator() *barkus.Generator {
	strictOnce.Do(func() {
		g, err := barkus.NewGeneratorWithOptions(Grammar,
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
		var v any
		_ = yaml.Unmarshal(out, &v)
	})
}
