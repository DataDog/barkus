package harness

import (
	"testing"

	pg_query "github.com/pganalyze/pg_query_go/v6"
)

func FuzzRaw(f *testing.F) {
	f.Fuzz(func(t *testing.T, data []byte) {
		_, _ = pg_query.Parse(string(data))
	})
}
