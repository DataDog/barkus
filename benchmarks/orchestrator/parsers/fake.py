"""Synthetic engine used for the orchestrator self-test.

Emits a deterministic but plausible-looking timeline given a seed and
elapsed time, so the schema + collector + writer pipeline can be exercised
end-to-end without requiring a real fuzz engine.
"""

from __future__ import annotations

import random


def synthetic_sample(seed: int, t_s: int) -> dict:
    """Return one sample for the given (seed, t_s) coordinate.

    Deterministic in (seed, t_s) so two smoke runs with the same seed
    produce bit-identical samples.jsonl.
    """
    rng = random.Random(seed * 1_000_003 + t_s)
    base_eps = 50_000 + seed * 1_000
    eps = base_eps + rng.randint(-2_000, 2_000)
    return {
        "t_s": t_s,
        "edges": min(2_000, t_s * 17 + seed * 5),
        "execs": eps * t_s,
        "execs_per_sec": eps,
        "crashes": 0,
        "rss_mb": 12 + (t_s // 10),
    }
