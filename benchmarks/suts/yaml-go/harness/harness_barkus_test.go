package harness

import (
	"sync"
	"testing"

	barkus "github.com/DataDog/barkus/go/pkg/barkus"
	yaml "gopkg.in/yaml.v3"
)

var (
	barkusMu  sync.Mutex
	barkusMap = map[barkus.ValidityMode]*barkus.Generator{}
)

func barkusGen(mode barkus.ValidityMode) *barkus.Generator {
	barkusMu.Lock()
	defer barkusMu.Unlock()
	if g, ok := barkusMap[mode]; ok {
		return g
	}
	g, err := barkus.NewGeneratorWithOptions(Grammar,
		barkus.WithValidityMode(mode),
		barkus.WithMaxDepth(24),
	)
	if err != nil {
		panic(err)
	}
	barkusMap[mode] = g
	return g
}

func fuzzBarkus(f *testing.F, mode barkus.ValidityMode) {
	g := barkusGen(mode)
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

func FuzzBarkusStrict(f *testing.F)    { fuzzBarkus(f, barkus.Strict) }
func FuzzBarkusNearvalid(f *testing.F) { fuzzBarkus(f, barkus.NearValid) }
func FuzzBarkusHavoc(f *testing.F)     { fuzzBarkus(f, barkus.Havoc) }
