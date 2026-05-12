/*
 * libxml2 fuzz harness — single source, four binaries via -DBARKUS_VARIANT.
 *
 * NOTE on engine choice (M6 deviation from plan, documented in PROGRESS.md):
 *   The plan picked AFL++ for C/C++ SUTs. AFL++ v4.21c does not build
 *   against the host's LLVM 20 (template incompatibilities) and the
 *   non-LLVM gcc/asm fallback hits a PATH self-recursion bug on this
 *   host. Rather than block M6, the harness ships as a libFuzzer target
 *   (clang -fsanitize=fuzzer), reusing M5's libFuzzer engine path in the
 *   orchestrator. Same instrumentation as the Rust SUTs, so edges are
 *   even more directly comparable across C and Rust. Dockerfile.c-suts
 *   can still build the AFL++ variant when LLVM 17 is available.
 *
 * libFuzzer entrypoint: LLVMFuzzerTestOneInput(data, size).
 *
 * Each barkus_* binary picks a ValidityMode via -DBARKUS_PROFILE=N at
 * compile time, then passes that to barkus_compile_with_config so the
 * three variants produce genuinely distinct output streams.
 */

#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <libxml/parser.h>

/* Barkus C ABI subset. */
typedef struct Handle Handle;
extern Handle *barkus_compile_with_config(const uint8_t *source, size_t source_len,
                                          const uint8_t *config_json,
                                          size_t config_json_len,
                                          uint64_t seed);
extern int32_t barkus_decode(Handle *handle, const uint8_t *tape, size_t tape_len,
                             uint8_t *output_buf, size_t *output_len);
extern void barkus_destroy(Handle *handle);
extern const char *barkus_last_error(void);

#ifdef BARKUS_VARIANT
static const char XML_GRAMMAR[] =
    "start = element ;\n"
    "element = open content close | empty ;\n"
    "open = \"<\" name \">\" ;\n"
    "close = \"</\" name \">\" ;\n"
    "empty = \"<\" name \"/>\" ;\n"
    "content = element content | text content | \"\" ;\n"
    "text = \"hello\" | \"world\" | \"x\" ;\n"
    "name = \"a\" | \"b\" | \"node\" | \"item\" ;\n";

/* Pick a config_json based on BARKUS_PROFILE (0 = Strict, 1 = NearValid,
 * 2 = Havoc). build.sh wires this via -DBARKUS_PROFILE=<N>. */
#ifndef BARKUS_PROFILE
#define BARKUS_PROFILE 0
#endif
#if BARKUS_PROFILE == 0
static const char BARKUS_CONFIG[] = "{\"validity_mode\":\"Strict\",\"max_depth\":16}";
#elif BARKUS_PROFILE == 1
static const char BARKUS_CONFIG[] = "{\"validity_mode\":\"NearValid\",\"max_depth\":16}";
#elif BARKUS_PROFILE == 2
static const char BARKUS_CONFIG[] = "{\"validity_mode\":\"Havoc\",\"max_depth\":16}";
#else
#error "BARKUS_PROFILE must be 0 (Strict), 1 (NearValid), or 2 (Havoc)"
#endif

static Handle *g_handle = NULL;
#endif

int LLVMFuzzerInitialize(int *argc, char ***argv) {
    (void)argc; (void)argv;
    xmlInitParser();
#ifdef BARKUS_VARIANT
    g_handle = barkus_compile_with_config(
        (const uint8_t *)XML_GRAMMAR, sizeof(XML_GRAMMAR) - 1,
        (const uint8_t *)BARKUS_CONFIG, sizeof(BARKUS_CONFIG) - 1,
        /*seed=*/0);
    if (!g_handle) {
        const char *err = barkus_last_error();
        fprintf(stderr, "barkus_compile_with_config failed: %s\n", err ? err : "?");
        return 1;
    }
#endif
    return 0;
}

int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size) {
    if (size == 0) return 0;
#ifdef BARKUS_VARIANT
    static uint8_t out_buf[1 << 16];
    size_t out_len = sizeof(out_buf);
    if (barkus_decode(g_handle, data, size, out_buf, &out_len) != 0) return 0;
    if (out_len == 0) return 0;
    xmlDocPtr doc = xmlReadMemory((const char *)out_buf, (int)out_len,
                                  "in.xml", NULL, 0);
#else
    xmlDocPtr doc = xmlReadMemory((const char *)data, (int)size,
                                  "in.xml", NULL, 0);
#endif
    if (doc) xmlFreeDoc(doc);
    return 0;
}
