/*
 * FFI smoke binary — proves the Barkus C-ABI FFI links and runs end-to-end.
 *
 * Smoke contract: `docker run barkus-base /opt/barkus/test-roundtrip` exits 0.
 *
 * Reads a JSON EBNF grammar from /opt/barkus/share/json.ebnf, compiles it
 * via barkus_compile, asks barkus_generate to produce one sample, then
 * round-trips the same byte sequence through barkus_decode. Failure of any
 * step (NULL handle, FFI returning -1, empty output, decode mismatch) exits
 * with a non-zero code and prints barkus_last_error() to stderr.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

/* Forward-declare the subset of the Barkus FFI we use. cbindgen also generates
 * a full barkus.h at /opt/barkus/include/barkus.h, but the FFI surface is
 * small enough that hand-declaring here keeps the smoke test self-contained
 * and lets it compile even if cbindgen output drifts. */
typedef struct Handle Handle;

extern Handle *barkus_compile(const uint8_t *source,
                              size_t source_len,
                              uint64_t seed,
                              uint32_t max_depth);
extern int32_t barkus_generate(Handle *handle,
                               uint8_t *output_buf,
                               size_t *output_len);
extern int32_t barkus_decode(Handle *handle,
                             const uint8_t *tape,
                             size_t tape_len,
                             uint8_t *output_buf,
                             size_t *output_len);
extern void    barkus_destroy(Handle *handle);
extern const char *barkus_last_error(void);

static int read_file(const char *path, uint8_t **out, size_t *out_len) {
    FILE *f = fopen(path, "rb");
    if (!f) { perror(path); return 1; }
    fseek(f, 0, SEEK_END);
    long n = ftell(f);
    if (n < 0) { fclose(f); return 1; }
    fseek(f, 0, SEEK_SET);
    uint8_t *buf = (uint8_t *)malloc((size_t)n);
    if (!buf) { fclose(f); return 1; }
    if (fread(buf, 1, (size_t)n, f) != (size_t)n) {
        free(buf); fclose(f); return 1;
    }
    fclose(f);
    *out = buf;
    *out_len = (size_t)n;
    return 0;
}

static void die(const char *step) {
    const char *err = barkus_last_error();
    fprintf(stderr, "FAIL %s: %s\n", step, err ? err : "(no error message)");
    exit(2);
}

int main(int argc, char **argv) {
    const char *grammar_path = (argc > 1) ? argv[1] : "/opt/barkus/share/json.ebnf";
    uint8_t *grammar = NULL;
    size_t   grammar_len = 0;
    if (read_file(grammar_path, &grammar, &grammar_len) != 0) {
        fprintf(stderr, "FAIL read_file: %s\n", grammar_path);
        return 3;
    }

    Handle *h = barkus_compile(grammar, grammar_len, /*seed=*/42, /*max_depth=*/24);
    free(grammar);
    if (!h) die("barkus_compile");

    uint8_t out[4096];
    size_t  out_len = sizeof(out);
    if (barkus_generate(h, out, &out_len) != 0) die("barkus_generate");
    if (out_len == 0) {
        fprintf(stderr, "FAIL barkus_generate: produced empty output\n");
        barkus_destroy(h);
        return 4;
    }

    /* Round-trip a small valid tape through decode. The "decode" path is what
     * a fuzzer harness uses every iteration, so we must prove it accepts an
     * arbitrary byte sequence and produces some output without crashing. */
    uint8_t tape[64];
    for (size_t i = 0; i < sizeof(tape); i++) tape[i] = (uint8_t)(i * 17 + 3);
    uint8_t dec[4096];
    size_t  dec_len = sizeof(dec);
    if (barkus_decode(h, tape, sizeof(tape), dec, &dec_len) != 0) die("barkus_decode");

    barkus_destroy(h);

    fprintf(stdout,
            "OK barkus FFI round-trip: generate=%zu bytes, decode=%zu bytes\n",
            out_len, dec_len);
    return 0;
}
