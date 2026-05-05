"""Aggregate per-cell run.json + samples.jsonl into a single parquet.

For a given <run-id> (smoke or full), walks
  results/<stage>/<run-id>/<sut>/<variant>/dict-<m>/seed-<n>/
and writes
  reports/<run-id>/results.parquet
  reports/<run-id>/summary.csv

results.parquet has one row per (sut, variant, dict, seed, t_s); summary.csv
has one row per (sut, variant, dict) with final metrics + Mann-Whitney U
p-values when there are at least 3 seeds (skipped on smoke runs that use
1 seed).
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Iterator

import pandas as pd
from scipy.stats import mannwhitneyu  # type: ignore[import-untyped]

from schema import Run, Sample


def _crash_artifacts(cell_dir: Path) -> list[Path]:
    """Yield crash artifact files for a cell. Engine-specific layouts:
        libfuzzer:    cwd/crash-<hash>, cwd/leak-<hash>, cwd/oom-<hash>
        Go testing.F: testdata/fuzz/<TestName>/<hash>
    """
    out: list[Path] = []
    for child in cell_dir.iterdir():
        n = child.name
        if child.is_file() and (n.startswith("crash-")
                                 or n.startswith("leak-")
                                 or n.startswith("oom-")):
            out.append(child)
    testdata = cell_dir / "testdata" / "fuzz"
    if testdata.is_dir():
        for fn in testdata.rglob("*"):
            if fn.is_file():
                out.append(fn)
    return out


def _byte_hash(path: Path) -> str:
    h = hashlib.sha256()
    h.update(path.read_bytes())
    return h.hexdigest()[:16]


def _dedup_crashes(cell_dir: Path) -> int:
    """Conservative content-based dedup. The plan calls for ASan-replay +
    symbolicated stack-hash dedup; that needs ASan-instrumented sibling
    binaries (BARKUS_SAN=1 builds) which we don't keep alongside production
    binaries today. Until that lands, group by file-content sha256 — gives
    a strict upper bound on unique crashes (each unique file is at least
    one unique crash; the real number is ≤ this)."""
    artifacts = _crash_artifacts(cell_dir)
    if not artifacts:
        return 0
    return len({_byte_hash(p) for p in artifacts})


def _walk_cells(run_dir: Path) -> Iterator[Path]:
    """Yield every seed-<n> directory under a run-id's results tree."""
    if not run_dir.exists():
        return
    for sut_dir in run_dir.iterdir():
        if not sut_dir.is_dir():
            continue
        for variant_dir in sut_dir.iterdir():
            if not variant_dir.is_dir():
                continue
            for dict_dir in variant_dir.iterdir():
                if not dict_dir.is_dir() or not dict_dir.name.startswith("dict-"):
                    continue
                for seed_dir in dict_dir.iterdir():
                    if seed_dir.is_dir() and seed_dir.name.startswith("seed-"):
                        yield seed_dir


def _read_cell(cell_dir: Path) -> tuple[Run, list[Sample]]:
    run = Run.model_validate_json((cell_dir / "run.json").read_text())
    samples: list[Sample] = []
    for line in (cell_dir / "samples.jsonl").read_text().splitlines():
        if not line.strip():
            continue
        samples.append(Sample.model_validate_json(line))
    return run, samples


def aggregate(run_id: str, results_root: Path, reports_root: Path) -> Path:
    """Aggregate one run-id. Returns path to results.parquet."""
    # Locate the run dir under results/{smoke,full}/<run-id>/.
    candidates = [results_root / stage / run_id for stage in ("smoke", "full")]
    run_dirs = [d for d in candidates if d.exists()]
    if not run_dirs:
        raise FileNotFoundError(
            f"run-id {run_id} not found under {results_root}/{{smoke,full}}/"
        )
    if len(run_dirs) > 1:
        raise RuntimeError(f"ambiguous run-id {run_id}: matches {run_dirs}")
    run_dir = run_dirs[0]

    rows: list[dict] = []
    summary_rows: list[dict] = []
    for cell in _walk_cells(run_dir):
        run, samples = _read_cell(cell)
        for s in samples:
            rows.append({
                "run_id": run.run_id,
                "tier": run.tier,
                "sut": run.sut,
                "variant": run.variant,
                "dict": run.dict_mode,
                "seed": run.seed,
                "engine": run.engine,
                "barkus_sha": run.barkus_sha,
                "sut_sha": run.sut_sha,
                "t_s": s.t_s,
                "edges": s.edges,
                "execs": s.execs,
                "execs_per_sec": s.execs_per_sec,
                "crashes": s.crashes,
                "rss_mb": s.rss_mb,
            })
        summary_rows.append({
            "sut": run.sut,
            "variant": run.variant,
            "dict": run.dict_mode,
            "seed": run.seed,
            "engine": run.engine,
            "duration_s": run.duration_s,
            "final_edges": run.final.edges,
            "final_execs": run.final.execs,
            "final_execs_per_sec": run.final.execs_per_sec,
            "final_crashes_unique": run.final.crashes_unique_engine,
            "crashes_unique_dedup": _dedup_crashes(cell),
            "time_to_first_crash_s": run.final.time_to_first_crash_s,
        })

    if not rows:
        raise RuntimeError(f"no cells found under {run_dir}")

    out_dir = reports_root / run_id
    out_dir.mkdir(parents=True, exist_ok=True)

    df = pd.DataFrame(rows)
    parquet_path = out_dir / "results.parquet"
    df.to_parquet(parquet_path, index=False)

    summary = pd.DataFrame(summary_rows)
    # Write per-seed summary; report.py aggregates across seeds when rendering
    # so all the stats (mean ± std, p-values) come from the same source rows.
    summary.to_csv(out_dir / "summary.csv", index=False)

    # Mann-Whitney U vs raw, for each (sut, variant != raw) — skipped if
    # n_seeds < 3 since the test is meaningless on tiny samples.
    pvals: list[dict] = []
    for sut, sut_df in summary.groupby("sut"):
        raw_edges = sut_df[sut_df.variant == "raw"]["final_edges"].values
        if len(raw_edges) < 3:
            continue
        for variant, var_df in sut_df.groupby("variant"):
            if variant == "raw":
                continue
            v_edges = var_df["final_edges"].values
            if len(v_edges) < 3:
                continue
            try:
                stat, p = mannwhitneyu(v_edges, raw_edges, alternative="greater")
            except ValueError:
                continue
            pvals.append({"sut": sut, "variant": variant, "U": float(stat), "p_vs_raw": float(p)})
    if pvals:
        pd.DataFrame(pvals).to_csv(out_dir / "pvalues.csv", index=False)

    return parquet_path
