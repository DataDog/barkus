"""Parser for AFL++ output.

AFL++ writes a key-value `fuzzer_stats` file in the run output directory
every 30s. The orchestrator reads it directly each sample period; there
is no stderr-line parsing for AFL++.

Relevant keys (one int value per line, `key  : value`):
    start_time, last_update, run_time, cycles_done,
    execs_done, execs_per_sec, paths_total, edges_found,
    saved_crashes, last_crash, last_hang

"""

from __future__ import annotations

import re
from pathlib import Path
from typing import Optional

_KV_RE = re.compile(r"^([\w_]+)\s*:\s*(\S+)\s*$")


def read_stats(stats_path: Path) -> Optional[dict]:  # pragma: no cover
    """Read `fuzzer_stats` and return a normalized sample dict, or None."""
    try:
        text = stats_path.read_text()
    except FileNotFoundError:
        return None
    kv: dict[str, str] = {}
    for line in text.splitlines():
        m = _KV_RE.match(line)
        if m:
            kv[m.group(1)] = m.group(2)
    if "execs_done" not in kv:
        return None
    return {
        "execs": int(kv["execs_done"]),
        "execs_per_sec": int(float(kv.get("execs_per_sec", "0"))),
        "edges": int(kv.get("edges_found", "0")),
        "crashes": int(kv.get("saved_crashes", "0")),
    }
