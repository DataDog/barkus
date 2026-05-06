// Package barkus provides Go bindings for the barkus grammar fuzzer.
package barkus

/*
#cgo LDFLAGS: ${SRCDIR}/../../../target/release/libbarkus_ffi.a
#cgo darwin LDFLAGS: -framework Security
#include <stdint.h>
#include <stdlib.h>

extern void* barkus_compile(const uint8_t *source, size_t source_len,
                            uint64_t seed, uint32_t max_depth);
extern void* barkus_compile_with_config(const uint8_t *source, size_t source_len,
                            const uint8_t *config_json, size_t config_json_len,
                            uint64_t seed);
extern int barkus_generate(void *handle,
                           uint8_t *output_buf, size_t *output_len);
extern int barkus_generate_with_tape(void *handle,
                           uint8_t *output_buf, size_t *output_len,
                           uint8_t *tape_buf, size_t *tape_len);
extern int barkus_decode(void *handle,
                         const uint8_t *tape_ptr, size_t tape_len,
                         uint8_t *output_buf, size_t *output_len);
extern void barkus_destroy(void *handle);
extern const char* barkus_last_error();
*/
import "C"

import (
	"encoding/json"
	"errors"
	"runtime"
	"unsafe"
)

type GrammarFormat string

const (
	GrammarEbnf  GrammarFormat = "ebnf"
	GrammarAntlr GrammarFormat = "antlr"
	GrammarPeg   GrammarFormat = "peg"
)

type grammarConfig struct {
	commonConfig
	format *GrammarFormat
}

type grammarOption func(*grammarConfig)

func (o grammarOption) applyGrammar(c *grammarConfig) { o(c) }

// WithFormat selects the grammar source format. Default is GrammarEbnf.
func WithFormat(f GrammarFormat) GrammarOption {
	return grammarOption(func(c *grammarConfig) { c.format = &f })
}

// Wire shape consumed by Rust's GrammarConfig — field names must match.
// It's JSON serializable so it can be passed as a string through the FFI.
type grammarConfigJSON struct {
	MaxDepth      *uint32        `json:"max_depth,omitempty"`
	MaxTotalNodes *uint32        `json:"max_total_nodes,omitempty"`
	ValidityMode  *ValidityMode  `json:"validity_mode,omitempty"`
	Format        *GrammarFormat `json:"format,omitempty"`
}

func buildGrammarConfigJSON(cfg *grammarConfig) ([]byte, error) {
	if cfg.maxDepth == nil && cfg.maxTotalNodes == nil &&
		cfg.validityMode == nil && cfg.format == nil {
		return nil, nil
	}
	return json.Marshal(grammarConfigJSON{
		MaxDepth:      cfg.maxDepth,
		MaxTotalNodes: cfg.maxTotalNodes,
		ValidityMode:  cfg.validityMode,
		Format:        cfg.format,
	})
}

// Generator compiles a grammar and generates samples from it.
type Generator struct {
	handle unsafe.Pointer
}

// NewGenerator compiles the given grammar source and returns a Generator.
// seed controls the RNG (0 = random); maxDepth = 0 uses the default (30).
// For ValidityMode or grammar format control, use NewGeneratorWithOptions.
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

// NewGeneratorWithOptions compiles a grammar with the given options.
func NewGeneratorWithOptions(source string, opts ...GrammarOption) (*Generator, error) {
	var cfg grammarConfig
	for _, o := range opts {
		o.applyGrammar(&cfg)
	}

	configJSON, err := buildGrammarConfigJSON(&cfg)
	if err != nil {
		return nil, err
	}

	src := []byte(source)
	var srcPtr *C.uint8_t
	if len(src) > 0 {
		srcPtr = (*C.uint8_t)(unsafe.Pointer(&src[0]))
	}
	var configPtr *C.uint8_t
	if len(configJSON) > 0 {
		configPtr = (*C.uint8_t)(unsafe.Pointer(&configJSON[0]))
	}

	handle := C.barkus_compile_with_config(
		srcPtr, C.size_t(len(src)),
		configPtr, C.size_t(len(configJSON)),
		C.uint64_t(cfg.seed),
	)
	runtime.KeepAlive(src)
	runtime.KeepAlive(configJSON)

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

// GenerateWithTape produces one sample like Generate, but also writes the
// decision tape into tapeBuf. Returns sub-slices of both buffers.
func (g *Generator) GenerateWithTape(buf, tapeBuf []byte) (output, tape []byte, err error) {
	if g.handle == nil {
		return nil, nil, errors.New("barkus: generator is closed")
	}
	if len(buf) == 0 {
		return nil, nil, errors.New("barkus: buffer is empty")
	}
	if len(tapeBuf) == 0 {
		return nil, nil, errors.New("barkus: tape buffer is empty")
	}

	outputLen := C.size_t(len(buf))
	tapeLen := C.size_t(len(tapeBuf))
	rc := C.barkus_generate_with_tape(
		g.handle,
		(*C.uint8_t)(unsafe.Pointer(&buf[0])), &outputLen,
		(*C.uint8_t)(unsafe.Pointer(&tapeBuf[0])), &tapeLen,
	)
	runtime.KeepAlive(buf)
	runtime.KeepAlive(tapeBuf)

	if rc != 0 {
		return nil, nil, lastError()
	}
	return buf[:outputLen], tapeBuf[:tapeLen], nil
}

// Decode replays a decision tape against the given grammar.
//
// Stateless: compiles the grammar, decodes, and frees the handle on every
// call. Suitable for one-shot scripts; for hot loops (fuzz harnesses, RPC
// servers) compile a Generator once via NewGeneratorWithOptions and call
// (*Generator).Decode — that avoids the per-call compile cost.
//
// For ValidityMode control use DecodeWithOptions.
func Decode(source string, tape []byte, maxDepth uint32) ([]byte, error) {
	src := []byte(source)
	var srcPtr *C.uint8_t
	if len(src) > 0 {
		srcPtr = (*C.uint8_t)(unsafe.Pointer(&src[0]))
	}

	handle := C.barkus_compile(srcPtr, C.size_t(len(src)), C.uint64_t(0), C.uint32_t(maxDepth))
	runtime.KeepAlive(src)
	if handle == nil {
		return nil, lastError()
	}
	defer C.barkus_destroy(handle)

	return decodeWithHandle(handle, tape)
}

// DecodeWithOptions is Decode with options. Same compile-per-call cost —
// hot-loop callers should reuse a Generator via (*Generator).Decode.
func DecodeWithOptions(source string, tape []byte, opts ...GrammarOption) ([]byte, error) {
	var cfg grammarConfig
	for _, o := range opts {
		o.applyGrammar(&cfg)
	}

	configJSON, err := buildGrammarConfigJSON(&cfg)
	if err != nil {
		return nil, err
	}

	src := []byte(source)
	var srcPtr *C.uint8_t
	if len(src) > 0 {
		srcPtr = (*C.uint8_t)(unsafe.Pointer(&src[0]))
	}
	var configPtr *C.uint8_t
	if len(configJSON) > 0 {
		configPtr = (*C.uint8_t)(unsafe.Pointer(&configJSON[0]))
	}

	handle := C.barkus_compile_with_config(
		srcPtr, C.size_t(len(src)),
		configPtr, C.size_t(len(configJSON)),
		C.uint64_t(cfg.seed),
	)
	runtime.KeepAlive(src)
	runtime.KeepAlive(configJSON)
	if handle == nil {
		return nil, lastError()
	}
	defer C.barkus_destroy(handle)

	return decodeWithHandle(handle, tape)
}

// Decode replays a tape into buf, returning the written sub-slice.
// Not safe for concurrent calls on a single Generator, the underlying
// handle owns mutable state. Use one Generator per goroutine.
func (g *Generator) Decode(tape, buf []byte) ([]byte, error) {
	if g.handle == nil {
		return nil, errors.New("barkus: generator is closed")
	}
	if len(tape) == 0 {
		return nil, errors.New("barkus: tape is empty")
	}
	if len(buf) == 0 {
		return nil, errors.New("barkus: buffer is empty")
	}

	outputLen := C.size_t(len(buf))
	rc := C.barkus_decode(
		g.handle,
		(*C.uint8_t)(unsafe.Pointer(&tape[0])), C.size_t(len(tape)),
		(*C.uint8_t)(unsafe.Pointer(&buf[0])), &outputLen,
	)
	runtime.KeepAlive(tape)
	runtime.KeepAlive(buf)

	if rc != 0 {
		return nil, lastError()
	}
	return buf[:outputLen], nil
}

func decodeWithHandle(handle unsafe.Pointer, tape []byte) ([]byte, error) {
	if len(tape) == 0 {
		return nil, errors.New("barkus: tape is empty")
	}

	buf := make([]byte, 64*1024)
	outputLen := C.size_t(len(buf))
	rc := C.barkus_decode(
		handle,
		(*C.uint8_t)(unsafe.Pointer(&tape[0])), C.size_t(len(tape)),
		(*C.uint8_t)(unsafe.Pointer(&buf[0])), &outputLen,
	)
	runtime.KeepAlive(tape)
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
