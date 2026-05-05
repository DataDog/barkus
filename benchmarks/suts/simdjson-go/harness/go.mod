module github.com/DataDog/barkus/benchmarks/suts/simdjson-go/harness

// Pinned Go toolchain — must match Dockerfile.base ARG GO_VERSION.
go 1.22

toolchain go1.26.1

require (
	github.com/DataDog/barkus v0.0.0
	github.com/minio/simdjson-go v0.4.5-0.20230311191656-d82c779820b2
)

require (
	github.com/klauspost/compress v1.15.15 // indirect
	github.com/klauspost/cpuid/v2 v2.2.3 // indirect
	golang.org/x/sys v0.0.0-20220704084225-05e143d24a9e // indirect
)

// The barkus module is loaded from the parent repo. In the Docker image the
// repo lives at /src/barkus and this harness lives at
// /src/barkus/benchmarks/suts/simdjson-go/harness, so ../../../.. resolves
// correctly. cgo directives in go/pkg/barkus expect target/release/
// libbarkus_ffi.a at the repo root — Dockerfile.base ensures it is there.
replace github.com/DataDog/barkus => ../../../..
