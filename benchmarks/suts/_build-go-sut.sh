#!/usr/bin/env bash
# Shared builder for Go SUT harnesses. Each suts/<sut>/build.sh sources
# this file and calls `build_go_sut`. Reads from the calling SUT's pin.txt:
#   go_module                : the Go module path
#   commit                   : SUT version (passed to `go get @<commit>`)
#   gofuzzheaders_commit (opt): if set, also `go get` go-fuzz-headers at that SHA

set -euo pipefail

build_go_sut() {
    local sut_root="$1"
    local harness_dir="${sut_root}/harness"

    source "${sut_root}/pin.txt"
    : "${go_module:?pin.txt missing go_module=}"
    : "${commit:?pin.txt missing commit=}"

    cd "${harness_dir}"
    # Defeat any host-level git insteadOf rule that rewrites https → ssh
    # (would break unauthenticated module fetches).
    export GIT_CONFIG_GLOBAL=/dev/null
    export GOPROXY="${GOPROXY:-https://proxy.golang.org,direct}"

    go get "${go_module}@${commit}"
    if [[ -n "${gofuzzheaders_commit:-}" ]]; then
        go get "github.com/AdaLogics/go-fuzz-headers@${gofuzzheaders_commit}"
    fi
    go mod tidy
    go vet ./...

    # `go test -fuzz=.` at build time injects coverage instrumentation so
    # the resulting binary fuzzes with edge guidance (a pre-built `go test
    # -c` binary without -fuzz produces uninstrumented mutation, useless
    # for benchmarking).
    go test -fuzz=. -c -o "${harness_dir}/harness.test"

    echo "OK $(basename "${sut_root}") harness: ${harness_dir}/harness.test"
}
