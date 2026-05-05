package harness

import (
	"testing"

	"vitess.io/vitess/go/vt/sqlparser"
)

// FuzzRaw is the honest baseline: feeds raw fuzzer-emitted bytes straight
// into vitess's sqlparser, with no grammar/struct layer in between.
func FuzzRaw(f *testing.F) {
	f.Fuzz(func(t *testing.T, data []byte) {
		_, _ = sqlparser.NewTestParser().Parse(string(data))
	})
}
