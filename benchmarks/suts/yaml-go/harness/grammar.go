package harness

// YAML grammar for barkus generation. Hand-rolled EBNF covering a substantial
// subset of YAML 1.2 — block + flow mappings/sequences, plain/single/double-
// quoted scalars (with escape sequences), anchors, aliases, type tags,
// document markers, and special values (null, booleans, special floats,
// hex/octal/scientific numerics).
//
// Why hand-rolled: there is no upstream YAML grammar in antlr/grammars-v4 to
// vendor. The YAML 1.2 spec uses parameterized BNF (`s-indent(n)`,
// context flags `block-in`/`flow-out`/etc.) which barkus-ebnf — a vanilla
// EBNF compiler with no parameters or lookahead — cannot express. Block
// nesting therefore goes through *flow* forms only; some structurally valid
// YAML is unreachable.
const Grammar = `
start = doc ;

doc = preamble doc_body trailer ;
preamble = "" | "---\n" ;
trailer = "" | "...\n" ;
doc_body = block_seq | block_map | flow_node "\n" ;

block_seq = block_seq_entry | block_seq_entry block_seq ;
block_seq_entry = "- " inline_node "\n" | "-\n" ;

block_map = block_map_entry | block_map_entry block_map ;
block_map_entry = block_key ": " inline_node "\n" ;
block_key = plain_word | sq_string | dq_string ;

inline_node = scalar | flow_seq | flow_map | tagged | anchored ;

tagged = tag " " inline_no_tag ;
inline_no_tag = scalar | flow_seq | flow_map | anchored ;

anchored = "&" anchor_name " " inline_no_anchor ;
inline_no_anchor = scalar | flow_seq | flow_map | tagged ;

alias = "*" anchor_name ;
anchor_name = "a" | "b" | "ref" | "x1" ;

tag = "!!str" | "!!int" | "!!float" | "!!bool" | "!!null"
    | "!!seq" | "!!map" | "!!binary" | "!" tag_name ;
tag_name = "custom" | "user" | "type" ;

scalar = plain_scalar | sq_string | dq_string | special_scalar ;
plain_scalar = plain_word | plain_word " " plain_word ;
plain_word = "hello" | "world" | "name" | "tag" | "value"
           | "abc" | "xyz" | "key1" | "key2" ;
special_scalar = "null" | "~" | "true" | "false" | "True" | "False"
               | "0" | "1" | "-1" | "42" | "0.5" | "3.14" | "-7"
               | "0x1F" | "0o7" | "1e3" | "-2.5e-3"
               | ".inf" | "-.inf" | ".nan" ;

sq_string = "'" sq_body "'" ;
sq_body = "" | "abc" | "hello world" | "it''s" | "a:b" | "1,2,3" ;

dq_string = "\"" dq_body "\"" ;
dq_body = "" | "abc" | "hi" | "hello\\nworld" | "tab\\there"
        | "\\u00e9" | "\\\"quote\\\"" | "back\\\\slash" ;

flow_seq = "[]" | "[" flow_seq_items "]" ;
flow_seq_items = flow_node | flow_node ", " flow_seq_items ;

flow_map = "{}" | "{" flow_map_items "}" ;
flow_map_items = flow_map_entry | flow_map_entry ", " flow_map_items ;
flow_map_entry = flow_key ": " flow_node ;
flow_key = plain_word | sq_string | dq_string ;

flow_node = scalar | flow_seq | flow_map | flow_tagged | flow_anchored | alias ;
flow_tagged = tag " " flow_no_tag ;
flow_no_tag = scalar | flow_seq | flow_map | flow_anchored | alias ;
flow_anchored = "&" anchor_name " " flow_no_anchor ;
flow_no_anchor = scalar | flow_seq | flow_map | flow_tagged ;
`
