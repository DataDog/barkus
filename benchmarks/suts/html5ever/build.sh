#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
source "${HERE}/../_build-rust-sut.sh"
build_rust_sut "${HERE}"
