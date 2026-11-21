"""Typed schemas for the Barkus benchmark orchestrator.

Two output artifacts per cell:
  - run.json     : single object capturing pre-registered metadata + final stats
  - samples.jsonl: one JSON object per 10-second sample (time-series)

Aggregate consumers read both shapes via these models.
"""

from __future__ import annotations

from typing import Literal, Optional

from pydantic import BaseModel, NonNegativeInt


# -- engine types ------------------------------------------------------------

EngineId = Literal[
    "go-testing-f",   # Go SUTs via `go test -fuzz`
    "libfuzzer",      # Rust SUTs via cargo-fuzz
    "aflpp",          # C/C++ SUTs via AFL++
    "fake",           # synthetic engine used for orchestrator self-test
]

DictMode = Literal["on", "off"]


# -- sub-records -------------------------------------------------------------

class HostInfo(BaseModel):
    cpu: str
    kernel: str
    cpu_pin: Optional[str] = None


class FinalStats(BaseModel):
    """Summary captured at end of cell (also derivable from samples.jsonl)."""

    edges: NonNegativeInt
    execs: NonNegativeInt
    execs_per_sec: NonNegativeInt
    crashes_unique_engine: NonNegativeInt
    crashes_unique_stack_hash: Optional[NonNegativeInt] = None
    time_to_first_crash_s: Optional[NonNegativeInt] = None


# -- top-level shapes --------------------------------------------------------

class Sample(BaseModel):
    """One row of samples.jsonl. Time-series cadence: every 10 seconds."""

    t_s: NonNegativeInt
    edges: NonNegativeInt
    execs: NonNegativeInt
    execs_per_sec: NonNegativeInt
    crashes: NonNegativeInt
    rss_mb: NonNegativeInt = 0


class Run(BaseModel):
    """The run.json artifact. Pre-registered metadata + final stats.

    Fields are append-only; new fields must default-None so older run.json
    files keep validating after schema bumps.
    """

    run_id: str
    tier: int
    sut: str
    variant: str
    seed: int
    dict_mode: DictMode

    engine: EngineId
    engine_version: str

    # Provenance — pinned before the cell starts. Reproducibility-critical.
    barkus_sha: str
    sut_sha: str = ""
    grammar_path: Optional[str] = None
    grammar_sha: Optional[str] = None

    duration_s: NonNegativeInt
    host: HostInfo
    corpus_seeded: bool = False

    final: FinalStats
