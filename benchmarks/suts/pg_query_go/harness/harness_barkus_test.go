package harness

import (
	"sync"
	"testing"

	barkus "github.com/DataDog/barkus/go/pkg/barkus"
	pg_query "github.com/pganalyze/pg_query_go/v6"
)

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
		barkus.PostgreSQL,
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
		_, _ = pg_query.Parse(string(sql))
	})
}

func FuzzBarkusStrict(f *testing.F)    { fuzzBarkus(f, barkus.Strict) }
func FuzzBarkusNearvalid(f *testing.F) { fuzzBarkus(f, barkus.NearValid) }
func FuzzBarkusHavoc(f *testing.F)     { fuzzBarkus(f, barkus.Havoc) }
