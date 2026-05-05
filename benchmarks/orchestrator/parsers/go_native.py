"""Parser for Go's native `testing.F` fuzz output.

Sample stderr lines emitted by `go test -fuzz=Fuzz`:

    fuzz: elapsed: 0s, gathering baseline coverage: 0/1 completed
    fuzz: elapsed: 1m0s, execs: 12345 (205/sec), new interesting: 3 (total: 7)
    fuzz: elapsed: 4h0m0s, execs: 980000000 (68000/sec), new interesting: 0 (total: 1234)

Crashes appear at `testdata/fuzz/<TestName>/...` and the corresponding
stderr message is `--- FAIL: ...`.

Implementation deferred to M3 (simdjson-go is the first Go SUT). For M2
this stub exposes a no-op parser so the orchestrator imports succeed.
"""

from __future__ import annotations

import re
from typing import Optional

# Group order matches the parsed line; values are running totals the
# orchestrator turns into per-sample deltas.
_RUNNING_RE = re.compile(
    r"fuzz: elapsed: (?P<elapsed>\S+),\s*"
    r"execs: (?P<execs>\d+)\s*\((?P<eps>\d+)/sec\),\s*"
    r"new interesting: \d+\s*\(total: (?P<edges>\d+)\)"
)


def parse_line(line: str, state: dict) -> Optional[dict]:  # pragma: no cover - filled in M3
    m = _RUNNING_RE.search(line)
    if not m:
        return None
    return {
        "execs": int(m["execs"]),
        "execs_per_sec": int(m["eps"]),
        "edges": int(m["edges"]),
        "crashes": state.get("crashes", 0),
    }
