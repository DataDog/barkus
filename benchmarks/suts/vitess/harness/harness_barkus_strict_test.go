package harness

import (
	"sync"
	"testing"

	barkus "github.com/DataDog/barkus/go/pkg/barkus"
	"vitess.io/vitess/go/vt/sqlparser"
)

// One SQLGenerator per ValidityMode, shared across all testing.F workers.
// SQLGenerator.Decode is concurrent-safe (the FFI takes a const handle and
// touches no mutable state on decode), and Generate is concurrent-safe via
// an internal RNG mutex on the Rust side.
var (
	strictOnce sync.Once
	strictGen  *barkus.SQLGenerator
)

func strictGenerator() *barkus.SQLGenerator {
	strictOnce.Do(func() {
		g, err := barkus.NewSQLGenerator(
			barkus.MySQL,
			barkus.WithValidityMode(barkus.Strict),
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
		sql, err := g.Decode(data, buf)
		if err != nil || len(sql) == 0 {
			return
		}
		_, _ = sqlparser.NewTestParser().Parse(string(sql))
	})
}
