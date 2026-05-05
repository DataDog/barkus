package harness

import (
	"testing"

	fuzz "github.com/AdaLogics/go-fuzz-headers"
	"vitess.io/vitess/go/vt/sqlparser"
)

// FuzzGoFuzzHeaders runs the AdaLogics go-fuzz-headers consumer over the
// fuzzer's input bytes and asks it for a SQL string, which is then parsed
// by vitess. This mirrors the pattern in vitess's own ast_fuzzer.go
// (go/test/fuzzing/ast_fuzzer.go) which uses GetSQLString as the input
// generator. It is the SOTA structure-aware baseline against which
// barkus_* variants compete.
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
		_, _ = sqlparser.NewTestParser().Parse(sql)
	})
}
