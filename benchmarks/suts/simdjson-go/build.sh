#!/usr/bin/env bash
# Validate the simdjson-go fuzz harness compiles and resolves deps.
#
# This step does NOT pre-compile a -c test binary because Go's fuzz
# coverage instrumentation is only injected during a `go test -fuzz=…`
# invocation; pre-built binaries fuzz without coverage and emit a warning.
# The orchestrator invokes `go test -fuzz=^FuzzX$ -fuzztime=60s` per cell,
# letting Go build an instrumented binary on each first invocation
# (subsequent invocations hit GOCACHE and are fast).

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
HARNESS_DIR="${HERE}/harness"

source "${HERE}/pin.txt"
: "${repo:?pin.txt missing repo=}"
: "${commit:?pin.txt missing commit=}"

cd "${HARNESS_DIR}"

go get "github.com/minio/simdjson-go@${commit}"
go mod tidy
go vet ./...

# Build an instrumented test binary. The `-fuzz=.` flag at build time tells
# Go's toolchain to inject coverage instrumentation into the binary so that
# `-test.fuzz=^X$` runs with edge guidance (without it, you get a warning
# and uninstrumented byte mutation, which is useless for benchmarking).
#
# The resulting harness.test exposes all FuzzX functions; each cell picks
# one via `-test.fuzz=^<func>$` at run time.
go test -fuzz=. -c -o "${HARNESS_DIR}/harness.test"

echo "OK simdjson-go harness: ${HARNESS_DIR}/harness.test"
