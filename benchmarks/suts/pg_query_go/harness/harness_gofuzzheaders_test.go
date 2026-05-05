package harness

import (
	"testing"

	fuzz "github.com/AdaLogics/go-fuzz-headers"
	pg_query "github.com/pganalyze/pg_query_go/v6"
)

func FuzzGoFuzzHeaders(f *testing.F) {
	f.Fuzz(func(t *testing.T, data []byte) {
		if len(data) < 10 {
			return
		}
		c := fuzz.NewConsumer(data)
		sql, err := c.GetSQLString()
		if err != nil {
			return
		}
		_, _ = pg_query.Parse(sql)
	})
}
