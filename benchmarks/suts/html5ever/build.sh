#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
source "${HERE}/pin.txt"
cd "${HERE}"
export GIT_CONFIG_GLOBAL=/dev/null
for target in barkus_strict barkus_nearvalid barkus_havoc arbitrary raw; do
    echo "==> cargo fuzz build ${target}"
    cargo "+${rust_nightly}" fuzz build --release "${target}"
done
echo "OK html5ever fuzz binaries built in fuzz/target/"
