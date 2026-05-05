package harness

import (
	"sync"
	"testing"

	barkus "github.com/DataDog/barkus/go/pkg/barkus"
	"vitess.io/vitess/go/vt/sqlparser"
)

var (
	nearvalidOnce sync.Once
	nearvalidGen  *barkus.SQLGenerator
)

func nearvalidGenerator() *barkus.SQLGenerator {
	nearvalidOnce.Do(func() {
		g, err := barkus.NewSQLGenerator(
			barkus.MySQL,
			barkus.WithValidityMode(barkus.NearValid),
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
		sql, err := g.Decode(data, buf)
		if err != nil || len(sql) == 0 {
			return
		}
		_, _ = sqlparser.NewTestParser().Parse(string(sql))
	})
}
