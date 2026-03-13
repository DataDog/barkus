# ADR-0008: SQL Semantic Generation Layer

## Status

Accepted — implementation in progress. See [ADR-0011](0011-semantic-hook-architecture.md) for the hook mechanism and [ADR-0012](0012-antlr-split-grammar-token-pools.md) for the split grammar/token pool strategy.

## Context and Problem Statement

A CFG can produce syntax-valid SQL, but not semantically meaningful queries. In fuzz-generators, the SQL generator uses a fixed pool of table/column names and builds AST nodes without type checking or scope tracking. How should barkus generate SQL that reaches deep execution paths beyond the parser?

## Decision Drivers

- Slutz, "Massive Stochastic Testing of SQL" (VLDB 1998): random SQL generation needs schema awareness.
- SQLsmith (Seltenreich, 2015): introspects live database schema before generating queries.
- SQLancer (Rigger & Su, ESEC/FSE 2020): multi-phase generation — schema → data → queries, with feedback via query plan diversity.
- Fuzz-generators' SQL generator supports only SELECT/INSERT/UPDATE/DELETE with a static table/column pool and max expression depth of 3.

## Considered Options

1. **Multi-phase semantic generation with pluggable context**
2. Expand the CFG with more SQL productions (syntax only)
3. Template-based SQL with holes
4. Full SQL dialect reimplementation per database

## Decision Outcome

Chosen option: **Option 1 — Multi-phase semantic generation with pluggable context.**

`barkus-sql` generates SQL in phases, each constrained by the output of the previous:

**Phase 1 — Schema context (provided or generated):**
- `SqlContext` holds: tables (name, columns with types), indexes, constraints, available functions.
- Can be provided by the user (from a live database introspection or static definition).
- Can be generated from a `SchemaProfile` for standalone fuzzing.

**Phase 2 — Query generation over the context:**
- **Symbol table**: table aliases, column references resolved against `SqlContext`.
- **Type-directed expression generation**: binary operators constrained by operand types (no `INT + VARCHAR`), function arguments matched to signatures.
- **Scope tracking**: subqueries, CTEs, and JOINs introduce new scopes; column references resolve against the current scope chain.
- **Complexity budget**: configurable limits on: number of JOINs, subquery nesting depth, expression depth, UNION arms, CTE count.

**Phase 3 — Dialect serialization:**
- Pluggable `SqlDialect` trait: Trino, PostgreSQL (double-quote quoting, `::` casts), SQLite, Generic.
- Keyword casing: Upper/Lower/Mixed (from profile).

**Semantic hooks for the core IR:**
`barkus-sql` registers semantic hooks ([ADR-0007](0007-configuration-and-profiles.md)) that run during generation:
- `resolve_identifier`: pick a column/table name from the current scope instead of a random string.
- `check_type_compat`: ensure expression type compatibility.
- `repair_scope`: fix dangling references after mutation.

**Integration with mutation ([ADR-0004](0004-structure-aware-mutation-strategy.md)):**
- In **Strict** mode: semantic repair runs after every AST mutation, ensuring all references resolve.
- In **NearValid** mode: semantic repair is skipped or selectively broken (e.g., reference a dropped table, use wrong column type).
- `FragmentDb` for SQL stores subtrees tagged with their type environment, enabling type-safe splicing.

### Pros

- Generates SQL that exercises query planners, optimizers, and execution engines — not just parsers.
- Schema-aware generation avoids the "valid syntax, immediate semantic error" problem.
- Pluggable context supports both standalone fuzzing and live-database-connected testing.
- Type-directed generation reduces wasted mutations.

### Cons

- Significantly more complex than a pure CFG generator.
- `SqlContext` must be kept in sync with the target database state for connected testing.
- Dialect-specific SQL features (window functions, recursive CTEs, JSON operators) need explicit modeling per dialect.

### Concrete Implementation Strategy

The semantic hook mechanism ([ADR-0011](0011-semantic-hook-architecture.md)) is the foundation for all three phases:

- **Phase 1** uses `on_token_pool` to override IDENTIFIER and literal pools with schema-derived values from `SqlContext`.
- **Phase 2** uses `enter_production`/`exit_production` for scope tracking (FROM clause, JOIN, subqueries) and `on_production` for type-directed expression generation.
- **Phase 3** uses `SqlDialect` trait implementations that are consulted by the hooks during token emission.

SQL grammars are compiled from vendored ANTLR split grammars ([ADR-0012](0012-antlr-split-grammar-token-pools.md)) via `compile_split()`, which maps lexer rules to `TokenPoolEntry` values in the IR. This gives hooks a natural interception point at the token level.

## Links

- [ADR-0011: Semantic Hook Architecture](0011-semantic-hook-architecture.md)
- [ADR-0012: ANTLR Split Grammar and Token Pool Strategy](0012-antlr-split-grammar-token-pools.md)
- Slutz, "Massive Stochastic Testing of SQL," VLDB 1998
- Seltenreich, "SQLsmith: A Random SQL Query Generator," 2015
- Rigger & Su, "Testing Database Engines via Pivoted Query Synthesis," ESEC/FSE 2020 (SQLancer)
