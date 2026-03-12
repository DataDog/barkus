# ADR-0008: SQL Semantic Generation Layer

## Status

Proposed

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
- Pluggable `SqlDialect` trait: MySQL (backtick quoting, LIMIT syntax), PostgreSQL (double-quote quoting, `::` casts), SQLite, Generic.
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

## Links

- Slutz, "Massive Stochastic Testing of SQL," VLDB 1998
- Seltenreich, "SQLsmith: A Random SQL Query Generator," 2015
- Rigger & Su, "Testing Database Engines via Pivoted Query Synthesis," ESEC/FSE 2020 (SQLancer)
