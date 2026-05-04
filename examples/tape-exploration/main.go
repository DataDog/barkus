// Command tape-exploration flips each body byte of a JSON decision tape and
// prints the decoded result, illustrating how single-byte changes map to
// targeted structural changes in the output.
package main

import (
	"encoding/hex"
	"fmt"
	"os"

	"github.com/DataDog/barkus/go/pkg/barkus"
)

// tapeHeaderBytes is the size of the fixed tape header. The first byte after
// the header is the first body byte we mutate.
const tapeHeaderBytes = 2

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}

func run() error {
	raw, err := os.ReadFile("fixtures/grammars/json.ebnf")
	if err != nil {
		return fmt.Errorf("read grammar (run from the repo root): %w", err)
	}
	grammar := string(raw)

	gen, err := barkus.NewGenerator(grammar, 42, 0)
	if err != nil {
		return fmt.Errorf("compile: %w", err)
	}
	defer gen.Close()

	buf := make([]byte, 64*1024)
	tapeBuf := make([]byte, 64*1024)

	out, tape, err := gen.GenerateWithTape(buf, tapeBuf)
	if err != nil {
		return fmt.Errorf("generate: %w", err)
	}

	fmt.Println("=== Original ===")
	fmt.Printf("output: %s\n", out)
	fmt.Printf("tape:   %s\n", hex.EncodeToString(tape))
	fmt.Printf("        (%d bytes: %d-byte header + %d body bytes)\n\n",
		len(tape), tapeHeaderBytes, len(tape)-tapeHeaderBytes)

	fmt.Println("=== Single-byte mutations ===")
	mutated := make([]byte, len(tape))
	copy(mutated, tape)
	for i := tapeHeaderBytes; i < len(tape); i++ {
		mutated[i] ^= 0x01
		decoded, err := barkus.Decode(grammar, mutated, 0)
		if err != nil {
			fmt.Printf("byte %2d (0x%02x → 0x%02x): decode error: %v\n",
				i, tape[i], mutated[i], err)
		} else {
			changed := ""
			if string(decoded) != string(out) {
				changed = " ← CHANGED"
			}
			fmt.Printf("byte %2d (0x%02x → 0x%02x): %s%s\n",
				i, tape[i], mutated[i], decoded, changed)
		}
		mutated[i] ^= 0x01
	}
	return nil
}
