module github.com/DataDog/barkus/benchmarks/suts/pg_query_go/harness

go 1.23

toolchain go1.26.2

require (
	github.com/AdaLogics/go-fuzz-headers v0.0.0-20240806141605-e8a1dd7889d6
	github.com/DataDog/barkus v0.0.0
	github.com/pganalyze/pg_query_go/v6 v6.2.2
)

require google.golang.org/protobuf v1.36.11 // indirect

replace github.com/DataDog/barkus => ../../../..
