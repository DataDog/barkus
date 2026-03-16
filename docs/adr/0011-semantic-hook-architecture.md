# ADR-0011: Semantic Hook Architecture

## Status

Proposed

## Context and Problem Statement

Grammar-driven generation ([ADR-0002](0002-decision-tape-and-havoc-paradox.md), [ADR-0003](0003-grammar-frontend-architecture.md)) produces syntactically valid output but cannot enforce semantic constraints. For SQL, an identifier must name a real table; a generated expression must type-check. How should barkus allow domain-specific logic to intercept and override generation decisions without coupling the core engine to any particular domain?

## Decision Drivers

- `barkus-core` must remain Sans I/O and domain-agnostic ([ADR-0010](0010-sans-io-and-tiger-style.md)).
- The mechanism must be zero-cost when no hooks are active — the common case for pure-grammar fuzzing must not pay for the SQL path.
- Hooks must compose with the decision tape: overridden decisions still consume tape bytes so that mutation and decode remain deterministic.
- The same mechanism should handle both production-level semantic overrides (e.g., "when generating an identifier, pick from the current scope") and token pool expansion (e.g., "the IDENTIFIER lexer rule should produce table names, not random strings").

## Considered Options

1. **Generic trait `SemanticHooks` with monomorphized dispatch, `()` as zero-cost default**
2. Dynamic dispatch via `Box<dyn SemanticHooks>`
3. Callback closures registered per production
4. Visitor pattern with pre/post hooks on every AST node

## Decision Outcome

Chosen option: **Option 1 — Generic trait with monomorphized dispatch.**

### The `SemanticHooks` trait

```rust
pub trait SemanticHooks {
    fn on_production(
        &mut self,
        hook_name: &str,
        tape_byte: u8,
        prod_id: ProductionId,
    ) -> Option<Vec<u8>>;

    fn on_token_pool(
        &mut self,
        pool_id: PoolId,
        tape_byte: u8,
    ) -> Option<Vec<u8>>;

    fn enter_production(&mut self, prod_id: ProductionId);
    fn exit_production(&mut self, prod_id: ProductionId);
}
```

**`on_production(hook_name, tape_byte, prod_id)`**: Called when expanding a production that has a `semantic_hook` attribute. The hook receives the tape byte (already consumed from the tape) and may return `Some(bytes)` to override the expansion entirely, or `None` to fall through to normal grammar-driven expansion. This is the primary mechanism for semantic overrides like "pick a column name from the current scope."

**`on_token_pool(pool_id, tape_byte)`**: Called when expanding a `TerminalKind::TokenPool`. The hook may return `Some(bytes)` to override the pool's mechanical expansion, or `None` to fall through to the default behavior (pick an alternative from the pool entry's alternatives using the tape byte). This unifies lexer expansion with semantic override — for example, an SQL hook can intercept the `IDENTIFIER` pool and substitute a real table name.

**`enter_production(prod_id)` / `exit_production(prod_id)`**: Called on entry/exit of every production expansion, regardless of whether the production has a semantic hook. These enable scope tracking (e.g., entering a `fromClause` pushes table aliases onto a scope stack).

### Zero-cost default

`impl SemanticHooks for ()` provides no-op defaults. All generation functions are generic over `H: SemanticHooks`, and existing callers pass `&mut ()`. Because Rust monomorphizes generics, the `()` instantiation compiles to the same code as before hooks existed — all hook call sites are eliminated by the optimizer.

### Tape integration

Hook invocations always consume a tape byte, even when the hook overrides the result. This ensures:
- The tape length is deterministic given a grammar + profile (independent of hook behavior).
- Mutation operators that modify tape bytes still affect hook-driven decisions.
- Decode with the same hooks produces the same output as generate.

### Pros

- Zero runtime cost when hooks are not used (monomorphization eliminates dead code).
- Single mechanism for production overrides, token pool expansion, and scope tracking.
- Composes cleanly with the decision tape and mutation engine.
- No dynamic dispatch overhead in the hot path.

### Cons

- Each distinct hook type creates a separate monomorphized copy of the generation code (binary size).
- Hook trait methods must be designed carefully — too many parameters or return types and the trait becomes unwieldy; too few and implementors need unsafe workarounds.
- Cannot swap hooks at runtime without an enum wrapper (acceptable: hook selection is a compile-time/startup decision).

## Links

- [ADR-0002: Decision Tape and Havoc Paradox](0002-decision-tape-and-havoc-paradox.md)
- [ADR-0008: SQL Semantic Generation Layer](0008-sql-semantic-generation-layer.md)
- [ADR-0010: Sans I/O and Tiger Style](0010-sans-io-and-tiger-style.md)
