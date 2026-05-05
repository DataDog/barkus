package harness

import (
	"sync"
	"testing"

	barkus "github.com/DataDog/barkus/go/pkg/barkus"
	pg_query "github.com/pganalyze/pg_query_go/v6"
)

var (
	havocOnce sync.Once
	havocGen  *barkus.SQLGenerator
)

func havocGenerator() *barkus.SQLGenerator {
	havocOnce.Do(func() {
		g, err := barkus.NewSQLGenerator(
			barkus.PostgreSQL,
			barkus.WithValidityMode(barkus.Havoc),
		)
		if err != nil {
			panic(err)
		}
		havocGen = g
	})
	return havocGen
}

func FuzzBarkusHavoc(f *testing.F) {
	g := havocGenerator()
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
