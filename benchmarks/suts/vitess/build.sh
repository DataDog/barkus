#!/usr/bin/env bash
# Build the vitess fuzz harness binary. Same shape as suts/simdjson-go/build.sh.

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
HARNESS_DIR="${HERE}/harness"

source "${HERE}/pin.txt"
: "${repo:?pin.txt missing repo=}"
: "${commit:?pin.txt missing commit=}"
: "${tag:?pin.txt missing tag=}"
: "${go_module:?pin.txt missing go_module=}"
: "${gofuzzheaders_commit:?pin.txt missing gofuzzheaders_commit=}"

cd "${HARNESS_DIR}"

# Resolve vitess at the pinned tag and go-fuzz-headers at the pinned SHA.
# GIT_CONFIG_GLOBAL=/dev/null disables host-level git insteadOf rewrites
# (some users have https://github.com/ → git@github.com: which fails for
# unauthenticated Go module fetches). GOPROXY prefers the public Go proxy
# but falls back to direct git for vanity domains the proxy doesn't have.
export GIT_CONFIG_GLOBAL=/dev/null
export GOPROXY="https://proxy.golang.org,direct"
export GOSUMDB="sum.golang.org"

# Pin by SHA — vitess.io/vitess module path doesn't follow Go's /vN scheme,
# so a tag like v24.0.0 is rejected. SHA always resolves regardless.
go get "${go_module}@${commit}"
go get "github.com/AdaLogics/go-fuzz-headers@${gofuzzheaders_commit}"
go mod tidy
go vet ./...

# Build the instrumented test binary (see notes in suts/simdjson-go/build.sh).
go test -fuzz=. -c -o "${HARNESS_DIR}/harness.test"

echo "OK vitess harness: ${HARNESS_DIR}/harness.test"
