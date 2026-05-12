package harness

import (
	"testing"

	yaml "gopkg.in/yaml.v3"
)

func FuzzRaw(f *testing.F) {
	f.Fuzz(func(t *testing.T, data []byte) {
		var v any
		_ = yaml.Unmarshal(data, &v)
	})
}
