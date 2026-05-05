#!/usr/bin/env bash
# Build the libxml2 fuzz harness binaries.
#
# Engine: clang's built-in libFuzzer via -fsanitize=fuzzer (deviation from
# the plan's AFL++ choice; see harness.c top comment for why). Same
# instrumentation as M5's Rust SUTs, so edges are comparable cross-engine
# within the libFuzzer family.
#
# Builds 4 binaries from a single harness.c:
#   barkus_strict, barkus_nearvalid, barkus_havoc — `-DBARKUS_VARIANT`
#                                                   `-DBARKUS_PROFILE=N`
#   raw                                            — no defines

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${HERE}/../../.." && pwd)"

source "${HERE}/pin.txt"

# Ensure libbarkus_ffi.a is built. Reuses the artifact M3 already produced.
if [[ ! -f "${REPO_ROOT}/target/release/libbarkus_ffi.a" ]]; then
    (cd "${REPO_ROOT}" && cargo build -p barkus-ffi --release)
fi
LIBBARKUS="${REPO_ROOT}/target/release/libbarkus_ffi.a"

XML_INCLUDE="$(pkg-config --cflags-only-I libxml-2.0 2>/dev/null || echo -I/usr/include/libxml2)"
XML_LIBS="$(pkg-config --libs libxml-2.0 2>/dev/null || echo -lxml2)"

CC="${CC:-clang}"
# BARKUS_SAN=0 (default) → libfuzzer-only; the timed M9 run uses this so
#                          execs/sec isn't crippled by ASan's 2-3× tax.
# BARKUS_SAN=1            → +AddressSanitizer; used by the post-hoc crash
#                          dedup pass and during harness development.
SAN_FLAGS="-fsanitize=fuzzer"
if [[ "${BARKUS_SAN:-0}" == "1" ]]; then
    SAN_FLAGS="-fsanitize=fuzzer,address"
fi
echo "using CC=${CC}  SAN_FLAGS=${SAN_FLAGS}"

build_one() {
    local out="$1" defs="$2"
    echo "==> ${CC} ${SAN_FLAGS} -> ${out}"
    "${CC}" -O2 -g -Wall \
        ${SAN_FLAGS} \
        ${defs} \
        ${XML_INCLUDE} \
        -o "${HERE}/${out}" \
        "${HERE}/harness.c" \
        "${LIBBARKUS}" \
        ${XML_LIBS} \
        -lpthread -ldl -lm
}

build_one "barkus_strict"    "-DBARKUS_VARIANT -DBARKUS_PROFILE=0"
build_one "barkus_nearvalid" "-DBARKUS_VARIANT -DBARKUS_PROFILE=1"
build_one "barkus_havoc"     "-DBARKUS_VARIANT -DBARKUS_PROFILE=2"
build_one "raw"              ""

echo "OK libxml2 fuzz binaries built in ${HERE}/"
