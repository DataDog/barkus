#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
source "${HERE}/../_build-go-sut.sh"
build_go_sut "${HERE}"
