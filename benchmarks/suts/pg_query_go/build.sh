#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
HARNESS_DIR="${HERE}/harness"
source "${HERE}/pin.txt"
cd "${HARNESS_DIR}"
export GIT_CONFIG_GLOBAL=/dev/null
export GOPROXY="https://proxy.golang.org,direct"
go get "${go_module}@${tag}"
go get "github.com/AdaLogics/go-fuzz-headers@${gofuzzheaders_commit}"
go mod tidy
go vet ./...
go test -fuzz=. -c -o "${HARNESS_DIR}/harness.test"
echo "OK pg_query_go harness: ${HARNESS_DIR}/harness.test"
