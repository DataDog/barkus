package barkus

/*
#cgo LDFLAGS: ${SRCDIR}/../../../target/release/libbarkus_ffi.a
#cgo darwin LDFLAGS: -framework Security
#include <stdint.h>
#include <stdlib.h>

extern void* barkus_sql_compile(const uint8_t *dialect, size_t dialect_len,
                                const uint8_t *config_json, size_t config_json_len,
                                uint64_t seed);
extern int barkus_sql_generate(void *handle,
                               uint8_t *output_buf, size_t *output_len);
extern int barkus_sql_generate_with_tape(void *handle,
                               uint8_t *output_buf, size_t *output_len,
                               uint8_t *tape_buf, size_t *tape_len);
extern int barkus_sql_decode(void *handle,
                             const uint8_t *tape_ptr, size_t tape_len,
                             uint8_t *output_buf, size_t *output_len);
extern void barkus_sql_destroy(void *handle);
extern const char* barkus_last_error();
*/
import "C"

import (
	"encoding/json"
	"errors"
	"runtime"
	"unsafe"
)

// Dialect identifies a SQL dialect for generation.
type Dialect string

const (
	PostgreSQL Dialect = "postgresql"
	SQLite     Dialect = "sqlite"
	Trino      Dialect = "trino"
	Generic    Dialect = "generic"
)

// SqlType represents a column data type.
type SqlType string

const (
	SqlInteger   SqlType = "integer"
	SqlFloat     SqlType = "float"
	SqlText      SqlType = "text"
	SqlBoolean   SqlType = "boolean"
	SqlTimestamp SqlType = "timestamp"
	SqlBlob      SqlType = "blob"
)

// Column describes a single table column.
type Column struct {
	Name     string  `json:"name"`
	Type     SqlType `json:"ty"`
	Nullable bool    `json:"nullable,omitempty"`
}

// Table describes a database table.
type Table struct {
	Name    string   `json:"name"`
	Columns []Column `json:"columns"`
}

// Schema describes the database schema for context-aware SQL generation.
type Schema struct {
	Tables []Table `json:"tables"`
}

// ValidityMode controls how strictly the generator respects grammar rules.
type ValidityMode string

const (
	Strict    ValidityMode = "Strict"
	NearValid ValidityMode = "NearValid"
	Havoc     ValidityMode = "Havoc"
)

// SQLOption configures a SQLGenerator.
type SQLOption func(*sqlConfig)

type sqlConfig struct {
	schema        *Schema
	schemaJSON    *string
	seed          uint64
	maxDepth      *uint32
	maxTotalNodes *uint32
	validityMode  *ValidityMode
}

// WithSchema sets the database schema for context-aware generation.
func WithSchema(s Schema) SQLOption {
	return func(c *sqlConfig) { c.schema = &s }
}

// WithSchemaJSON sets the schema from a raw JSON string.
func WithSchemaJSON(j string) SQLOption {
	return func(c *sqlConfig) { c.schemaJSON = &j }
}

// WithSeed sets the RNG seed for deterministic generation. 0 means random.
func WithSeed(seed uint64) SQLOption {
	return func(c *sqlConfig) { c.seed = seed }
}

// WithMaxDepth sets the maximum derivation depth.
func WithMaxDepth(depth uint32) SQLOption {
	return func(c *sqlConfig) { c.maxDepth = &depth }
}

// WithMaxTotalNodes sets the maximum number of AST nodes.
func WithMaxTotalNodes(n uint32) SQLOption {
	return func(c *sqlConfig) { c.maxTotalNodes = &n }
}

// WithValidityMode sets the validity mode.
func WithValidityMode(mode ValidityMode) SQLOption {
	return func(c *sqlConfig) { c.validityMode = &mode }
}

// SQLGenerator generates SQL strings using a compiled grammar and schema.
type SQLGenerator struct {
	handle unsafe.Pointer
}

// NewSQLGenerator creates a SQL generator for the given dialect.
func NewSQLGenerator(dialect Dialect, opts ...SQLOption) (*SQLGenerator, error) {
	var cfg sqlConfig
	for _, o := range opts {
		o(&cfg)
	}

	// Build config JSON blob.
	configJSON, err := buildConfigJSON(&cfg)
	if err != nil {
		return nil, err
	}

	// Dialect bytes.
	dialectBytes := []byte(dialect)
	var dialectPtr *C.uint8_t
	if len(dialectBytes) > 0 {
		dialectPtr = (*C.uint8_t)(unsafe.Pointer(&dialectBytes[0]))
	}

	// Config JSON bytes.
	var configPtr *C.uint8_t
	configLen := len(configJSON)
	if configLen > 0 {
		configPtr = (*C.uint8_t)(unsafe.Pointer(&configJSON[0]))
	}

	handle := C.barkus_sql_compile(
		dialectPtr, C.size_t(len(dialectBytes)),
		configPtr, C.size_t(configLen),
		C.uint64_t(cfg.seed),
	)
	runtime.KeepAlive(dialectBytes)
	runtime.KeepAlive(configJSON)

	if handle == nil {
		return nil, lastError()
	}

	g := &SQLGenerator{handle: handle}
	runtime.SetFinalizer(g, (*SQLGenerator).Close)
	return g, nil
}

// Generate produces one SQL string, writing into buf. Returns the sub-slice written.
func (g *SQLGenerator) Generate(buf []byte) ([]byte, error) {
	if g.handle == nil {
		return nil, errors.New("barkus: sql generator is closed")
	}
	if len(buf) == 0 {
		return nil, errors.New("barkus: buffer is empty")
	}

	outputLen := C.size_t(len(buf))
	rc := C.barkus_sql_generate(g.handle, (*C.uint8_t)(unsafe.Pointer(&buf[0])), &outputLen)
	runtime.KeepAlive(buf)

	if rc != 0 {
		return nil, lastError()
	}
	return buf[:outputLen], nil
}

// GenerateWithTape produces one SQL string and records the decision tape.
func (g *SQLGenerator) GenerateWithTape(buf, tapeBuf []byte) (output, tape []byte, err error) {
	if g.handle == nil {
		return nil, nil, errors.New("barkus: sql generator is closed")
	}
	if len(buf) == 0 {
		return nil, nil, errors.New("barkus: buffer is empty")
	}
	if len(tapeBuf) == 0 {
		return nil, nil, errors.New("barkus: tape buffer is empty")
	}

	outputLen := C.size_t(len(buf))
	tapeLen := C.size_t(len(tapeBuf))
	rc := C.barkus_sql_generate_with_tape(
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

// Decode replays a decision tape to reproduce the original SQL output.
func (g *SQLGenerator) Decode(tape, buf []byte) ([]byte, error) {
	if g.handle == nil {
		return nil, errors.New("barkus: sql generator is closed")
	}
	if len(tape) == 0 {
		return nil, errors.New("barkus: tape is empty")
	}
	if len(buf) == 0 {
		return nil, errors.New("barkus: buffer is empty")
	}

	outputLen := C.size_t(len(buf))
	rc := C.barkus_sql_decode(
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

// Close frees the underlying handle. Safe to call multiple times.
func (g *SQLGenerator) Close() {
	if g.handle != nil {
		C.barkus_sql_destroy(g.handle)
		g.handle = nil
		runtime.SetFinalizer(g, nil)
	}
}

// sqlConfigJSON is the JSON shape expected by the Rust FFI's SqlConfig.
type sqlConfigJSON struct {
	Schema        interface{}   `json:"schema,omitempty"`
	MaxDepth      *uint32       `json:"max_depth,omitempty"`
	MaxTotalNodes *uint32       `json:"max_total_nodes,omitempty"`
	ValidityMode  *ValidityMode `json:"validity_mode,omitempty"`
}

// buildConfigJSON marshals the sqlConfig options into a JSON blob for the FFI.
func buildConfigJSON(cfg *sqlConfig) ([]byte, error) {
	// If no options set, return nil (use Rust defaults).
	if cfg.schema == nil && cfg.schemaJSON == nil && cfg.maxDepth == nil &&
		cfg.maxTotalNodes == nil && cfg.validityMode == nil {
		return nil, nil
	}

	var c sqlConfigJSON

	if cfg.schemaJSON != nil {
		// Raw JSON passthrough — embed directly; Rust serde validates.
		c.Schema = json.RawMessage(*cfg.schemaJSON)
	} else if cfg.schema != nil {
		c.Schema = cfg.schema
	}

	c.MaxDepth = cfg.maxDepth
	c.MaxTotalNodes = cfg.maxTotalNodes
	c.ValidityMode = cfg.validityMode

	return json.Marshal(c)
}
