package harness

import (
	"sync"
	"testing"

	barkus "github.com/DataDog/barkus/go/pkg/barkus"
	"vitess.io/vitess/go/vt/sqlparser"
)

// One SQLGenerator per ValidityMode, lazily initialized. SQLGenerator.Decode
// is concurrent-safe (the FFI takes a const handle), and Generate is
// concurrent-safe via an internal RNG mutex on the Rust side.
var (
	barkusMu  sync.Mutex
	barkusMap = map[barkus.ValidityMode]*barkus.SQLGenerator{}
)

func barkusGen(mode barkus.ValidityMode) *barkus.SQLGenerator {
	barkusMu.Lock()
	defer barkusMu.Unlock()
	if g, ok := barkusMap[mode]; ok {
		return g
	}
	g, err := barkus.NewSQLGenerator(
		barkus.MySQL,
		barkus.WithValidityMode(mode),
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
		sql, err := g.Decode(data, buf)
		if err != nil || len(sql) == 0 {
			return
		}
		_, _ = sqlparser.NewTestParser().Parse(string(sql))
	})
}

func FuzzBarkusStrict(f *testing.F)    { fuzzBarkus(f, barkus.Strict) }
func FuzzBarkusNearvalid(f *testing.F) { fuzzBarkus(f, barkus.NearValid) }
func FuzzBarkusHavoc(f *testing.F)     { fuzzBarkus(f, barkus.Havoc) }
