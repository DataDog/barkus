module github.com/DataDog/barkus/benchmarks/suts/vitess/harness

go 1.26.2

require (
	github.com/AdaLogics/go-fuzz-headers v0.0.0-20240806141605-e8a1dd7889d6
	github.com/DataDog/barkus v0.0.0
	vitess.io/vitess v0.24.0
)

require (
	github.com/golang/glog v1.2.5 // indirect
	github.com/lmittmann/tint v1.1.3 // indirect
	github.com/mattn/go-isatty v0.0.21 // indirect
	github.com/planetscale/vtprotobuf v0.6.1-0.20250313105119-ba97887b0a25 // indirect
	github.com/spf13/pflag v1.0.10 // indirect
	golang.org/x/sys v0.47.0 // indirect
	google.golang.org/genproto/googleapis/rpc v0.0.0-20260526163538-3dc84a4a5aaa // indirect
	google.golang.org/grpc v1.83.0 // indirect
	google.golang.org/protobuf v1.36.11 // indirect
)

// vitess.io/vitess is added by build.sh via `go get @<sha>` which writes the
// pseudo-version automatically. We don't list it here because vitess does
// not follow Go's /vN major-version path scheme so it cannot be statically
// pinned at a tag in this require block.

// Local barkus — same pattern as the simdjson-go harness.
replace github.com/DataDog/barkus => ../../../..
