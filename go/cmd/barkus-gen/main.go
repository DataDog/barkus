// Command barkus-gen generates samples from an EBNF grammar using the barkus library.
package main

import (
	"encoding/hex"
	"flag"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/DataDog/barkus/go/pkg/barkus"
)

func main() {
	if len(os.Args) < 2 {
		printUsage()
		os.Exit(1)
	}

	switch os.Args[1] {
	case "generate":
		runGenerate(os.Args[2:])
	case "decode":
		runDecode(os.Args[2:])
	default:
		fmt.Fprintf(os.Stderr, "unknown subcommand: %s\n", os.Args[1])
		printUsage()
		os.Exit(1)
	}
}

func printUsage() {
	fmt.Fprintln(os.Stderr, "usage: barkus-gen <subcommand> [flags]")
	fmt.Fprintln(os.Stderr, "subcommands:")
	fmt.Fprintln(os.Stderr, "  generate  Generate samples from an EBNF grammar")
	fmt.Fprintln(os.Stderr, "  decode    Decode output from a decision tape")
}

func runGenerate(args []string) {
	fs := flag.NewFlagSet("generate", flag.ExitOnError)
	grammar := fs.String("grammar", "", "path to EBNF grammar file")
	count := fs.Int("count", 10, "number of samples to generate")
	seed := fs.Uint64("seed", 0, "RNG seed (0 = random)")
	maxDepth := fs.Uint("max-depth", 0, "max derivation depth (0 = default)")
	emitTape := fs.Bool("emit-tape", false, "emit hex-encoded tape to stderr")
	fs.Parse(args)

	if *grammar == "" {
		fmt.Fprintln(os.Stderr, "error: -grammar is required")
		fs.Usage()
		os.Exit(1)
	}

	source, err := os.ReadFile(*grammar)
	if err != nil {
		fmt.Fprintf(os.Stderr, "error reading %s: %v\n", *grammar, err)
		os.Exit(1)
	}

	gen, err := barkus.NewGenerator(string(source), *seed, uint32(*maxDepth))
	if err != nil {
		fmt.Fprintf(os.Stderr, "compile error: %v\n", err)
		os.Exit(1)
	}
	defer gen.Close()

	buf := make([]byte, 64*1024)
	tapeBuf := make([]byte, 64*1024)

	for i := 0; i < *count; i++ {
		if *emitTape {
			out, tape, err := gen.GenerateWithTape(buf, tapeBuf)
			if err != nil {
				fmt.Fprintf(os.Stderr, "generate error: %v\n", err)
				continue
			}
			fmt.Println(string(out))
			fmt.Fprintln(os.Stderr, hex.EncodeToString(tape))
		} else {
			out, err := gen.Generate(buf)
			if err != nil {
				fmt.Fprintf(os.Stderr, "generate error: %v\n", err)
				continue
			}
			fmt.Println(string(out))
		}
	}
}

func runDecode(args []string) {
	fs := flag.NewFlagSet("decode", flag.ExitOnError)
	grammar := fs.String("grammar", "", "path to EBNF grammar file")
	tapeFlag := fs.String("tape", "", "hex-encoded tape, or '-' for stdin")
	maxDepth := fs.Uint("max-depth", 0, "max derivation depth (0 = default)")
	fs.Parse(args)

	if *grammar == "" {
		fmt.Fprintln(os.Stderr, "error: -grammar is required")
		fs.Usage()
		os.Exit(1)
	}
	if *tapeFlag == "" {
		fmt.Fprintln(os.Stderr, "error: -tape is required")
		fs.Usage()
		os.Exit(1)
	}

	source, err := os.ReadFile(*grammar)
	if err != nil {
		fmt.Fprintf(os.Stderr, "error reading %s: %v\n", *grammar, err)
		os.Exit(1)
	}

	var tapeHex string
	if *tapeFlag == "-" {
		raw, err := io.ReadAll(os.Stdin)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error reading stdin: %v\n", err)
			os.Exit(1)
		}
		tapeHex = strings.TrimSpace(string(raw))
	} else {
		tapeHex = *tapeFlag
	}

	tapeBytes, err := hex.DecodeString(tapeHex)
	if err != nil {
		fmt.Fprintf(os.Stderr, "error decoding tape hex: %v\n", err)
		os.Exit(1)
	}

	out, err := barkus.Decode(string(source), tapeBytes, uint32(*maxDepth))
	if err != nil {
		fmt.Fprintf(os.Stderr, "decode error: %v\n", err)
		os.Exit(1)
	}

	fmt.Println(string(out))
}
