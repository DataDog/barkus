#!/usr/bin/env bash
# Build the resvg/usvg fuzz binaries via cargo-fuzz on pinned nightly.
#
# Same shape as suts/rust-cssparser/build.sh.

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
source "${HERE}/pin.txt"
: "${rust_nightly:?pin.txt missing rust_nightly=}"

cd "${HERE}"
export GIT_CONFIG_GLOBAL=/dev/null

for target in barkus_strict barkus_nearvalid barkus_havoc arbitrary raw; do
    echo "==> cargo fuzz build ${target}"
    cargo "+${rust_nightly}" fuzz build --release "${target}"
done

echo "OK resvg fuzz binaries built in fuzz/target/"
