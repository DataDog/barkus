"""Per-engine output parsers.

Each parser exposes:
    parse_line(line: str, state: dict) -> dict | None

`state` is a mutable dict the parser uses to accumulate counters between
lines (libFuzzer / Go's testing.F emit running totals; AFL++ writes a
fuzzer_stats file the parser reads in full).

Returning None means "no new sample to emit"; returning a dict means
"emit one sample now". The dict must be a valid `Sample` (see schema.py).
"""

from . import aflpp, fake, go_native, libfuzzer

__all__ = ["aflpp", "fake", "go_native", "libfuzzer"]
