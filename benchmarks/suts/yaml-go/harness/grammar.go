package harness

// YAML EBNF placeholder. Tiny subset (mappings + sequences + scalars +
// nested) — same shape as M5's CSS/SVG placeholders. Real yaml.g4 from
// antlr/grammars-v4 follow-up.
const Grammar = `
start = doc ;
doc = item | item "\n" doc ;
item = key ": " value ;
key = "name" | "id" | "tag" | "version" ;
value = scalar | "[" list "]" | "\n  " item ;
list = scalar | scalar "," list ;
scalar = "1" | "2" | "true" | "false" | "x" | "y" | "abc" ;
`
