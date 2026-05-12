package harness

import (
	"testing"

	"github.com/minio/simdjson-go"
)

// FuzzRaw is the honest baseline: feeds raw fuzzer-emitted bytes straight
// into simdjson-go's Parse, with no grammar layer in between. The plan
// names this the "raw" variant; it's the upstream-equivalent fuzz target
// used to measure how much the grammar variants help (or don't).
func FuzzRaw(f *testing.F) {
	f.Fuzz(func(t *testing.T, data []byte) {
		if len(data) == 0 {
			return
		}
		_, _ = simdjson.Parse(data, nil)
	})
}
