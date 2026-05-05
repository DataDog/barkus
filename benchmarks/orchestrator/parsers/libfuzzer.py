"""Parser for libFuzzer (cargo-fuzz) stderr.

Sample lines:

    #1234   NEW    cov: 567 ft: 890 corp: 12/345b lim: 64 exec/s: 1234 rss: 45Mb
    #2000   pulse  cov: 612 ft: 950 corp: 14/360b lim: 64 exec/s: 1300 rss: 47Mb
    ==12345==ERROR: AddressSanitizer: ...

Crashes are written as `crash-*` files in the artifact dir; ERROR lines
indicate one was just minted.

Stub for M2; M5 fills in the pulse/NEW handling.
"""

from __future__ import annotations

import re
from typing import Optional

_LINE_RE = re.compile(
    r"#(?P<execs>\d+)\s+\S+\s+"
    r"cov:\s*(?P<cov>\d+)\s+"
    r"ft:\s*(?P<ft>\d+)\s+"
    r"corp:\s*\d+/\d+\S*\s+"
    r"(?:lim:\s*\d+\s+)?"        # `lim:` is missing on the INITED line
    r"exec/s:\s*(?P<eps>\d+)\s+"
    r"rss:\s*(?P<rss>\d+)Mb"
)


def parse_line(line: str, state: dict) -> Optional[dict]:
    m = _LINE_RE.search(line)
    if not m:
        return None
    return {
        "execs": int(m["execs"]),
        "execs_per_sec": int(m["eps"]),
        "edges": int(m["cov"]),
        "rss_mb": int(m["rss"]),
        "crashes": state.get("crashes", 0),
    }
