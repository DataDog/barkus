#!/usr/bin/env bash
# Build the libxml2 fuzz harness binaries.
#
# Engine selection: driven by $CC.
#   afl-clang-fast (default inside barkus-c-suts image) → AFL++ instrumented;
#                          run via afl-fuzz.
#   clang (host fallback)  → libFuzzer (-fsanitize=fuzzer); run via
#                          -max_total_time. The host cannot build AFL++
#                          v4.21c against LLVM 20.
# Either path produces four binaries with the same names; the orchestrator
# picks the engine from config.yaml.
#
# BARKUS_SAN=1 layers AddressSanitizer on top (post-hoc crash dedup;
# 2-3× execs/sec tax — never use for timed runs).

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${HERE}/../../.." && pwd)"

source "${HERE}/pin.txt"

if [[ ! -f "${REPO_ROOT}/target/release/libbarkus_ffi.a" ]]; then
    (cd "${REPO_ROOT}" && cargo build -p barkus-ffi --release)
fi
LIBBARKUS="${REPO_ROOT}/target/release/libbarkus_ffi.a"

XML_INCLUDE="$(pkg-config --cflags-only-I libxml-2.0 2>/dev/null || echo -I/usr/include/libxml2)"
XML_LIBS="$(pkg-config --libs libxml-2.0 2>/dev/null || echo -lxml2)"

CC="${CC:-clang}"
case "${CC##*/}" in
    afl-clang-fast|afl-clang-lto)
        ENGINE_FLAGS=""   # afl-clang-fast supplies its own instrumentation
        ;;
    *)
        ENGINE_FLAGS="-fsanitize=fuzzer"
        ;;
esac
if [[ "${BARKUS_SAN:-0}" == "1" ]]; then
    ENGINE_FLAGS="${ENGINE_FLAGS},address"
    ENGINE_FLAGS="${ENGINE_FLAGS#,}"   # strip leading comma if AFL+ASan
fi
echo "using CC=${CC}  ENGINE_FLAGS=${ENGINE_FLAGS}"

build_one() {
    local out="$1" defs="$2"
    echo "==> ${CC} ${ENGINE_FLAGS} -> ${out}"
    "${CC}" -O2 -g -Wall \
        ${ENGINE_FLAGS} \
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
