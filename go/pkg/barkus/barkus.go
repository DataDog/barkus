// Package barkus provides Go bindings for the barkus EBNF fuzzer.
package barkus

/*
#cgo LDFLAGS: -L${SRCDIR}/../../../target/release -lbarkus_ffi
#cgo darwin LDFLAGS: -framework Security
#include <stdint.h>
#include <stdlib.h>

extern void* barkus_compile(const uint8_t *source, size_t source_len,
                            uint64_t seed, uint32_t max_depth);
extern int barkus_generate(void *handle,
                           uint8_t *output_buf, size_t *output_len);
extern void barkus_destroy(void *handle);
extern const char* barkus_last_error();
*/
import "C"

import (
	"errors"
	"runtime"
	"unsafe"
)

// Generator compiles an EBNF grammar and generates samples from it.
type Generator struct {
	handle unsafe.Pointer
}

// NewGenerator compiles the given EBNF source and returns a Generator.
// seed controls the RNG (0 = random). maxDepth overrides the default
// derivation depth limit (0 = default of 10).
func NewGenerator(source string, seed uint64, maxDepth uint32) (*Generator, error) {
	src := []byte(source)
	var srcPtr *C.uint8_t
	if len(src) > 0 {
		srcPtr = (*C.uint8_t)(unsafe.Pointer(&src[0]))
	}

	handle := C.barkus_compile(srcPtr, C.size_t(len(src)), C.uint64_t(seed), C.uint32_t(maxDepth))
	runtime.KeepAlive(src)

	if handle == nil {
		return nil, lastError()
	}

	g := &Generator{handle: handle}
	runtime.SetFinalizer(g, (*Generator).Close)
	return g, nil
}

// Generate produces one sample, writing into buf. It returns the sub-slice
// of buf that was written. The caller must provide a buffer large enough for
// the generated output.
func (g *Generator) Generate(buf []byte) ([]byte, error) {
	if g.handle == nil {
		return nil, errors.New("barkus: generator is closed")
	}
	if len(buf) == 0 {
		return nil, errors.New("barkus: buffer is empty")
	}

	outputLen := C.size_t(len(buf))
	rc := C.barkus_generate(g.handle, (*C.uint8_t)(unsafe.Pointer(&buf[0])), &outputLen)
	runtime.KeepAlive(buf)

	if rc != 0 {
		return nil, lastError()
	}
	return buf[:outputLen], nil
}

// Close frees the underlying handle. It is safe to call multiple times.
func (g *Generator) Close() {
	if g.handle != nil {
		C.barkus_destroy(g.handle)
		g.handle = nil
		runtime.SetFinalizer(g, nil)
	}
}

func lastError() error {
	p := C.barkus_last_error()
	if p == nil {
		return errors.New("barkus: unknown error")
	}
	return errors.New(C.GoString(p))
}
