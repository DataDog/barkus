// Command barkus-gen generates samples from an EBNF grammar using the barkus library.
package main

import (
	"flag"
	"fmt"
	"os"

	"github.com/DataDog/barkus/go/pkg/barkus"
)

func main() {
	grammar := flag.String("grammar", "", "path to EBNF grammar file")
	count := flag.Int("count", 10, "number of samples to generate")
	seed := flag.Uint64("seed", 0, "RNG seed (0 = random)")
	maxDepth := flag.Uint("max-depth", 0, "max derivation depth (0 = default)")
	flag.Parse()

	if *grammar == "" {
		fmt.Fprintln(os.Stderr, "error: -grammar is required")
		flag.Usage()
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

	buf := make([]byte, 64*1024) // 64 KiB output buffer
	for i := 0; i < *count; i++ {
		out, err := gen.Generate(buf)
		if err != nil {
			fmt.Fprintf(os.Stderr, "generate error: %v\n", err)
			continue
		}
		fmt.Println(string(out))
	}
}
