"""Aggregate per-cell run.json + samples.jsonl into one CSV.

For a given <run-id> (smoke or full), walks
  results/<stage>/<run-id>/<sut>/<variant>/dict-<m>/seed-<n>/
and writes
  reports/<run-id>/results.csv
  reports/<run-id>/summary.csv
  reports/<run-id>/pvalues.csv  (only when each variant has ≥3 seeds)

results.csv has one row per (sut, variant, dict_mode, seed, t_s); summary.csv
has one row per (sut, variant, dict_mode, seed) with final metrics.
"""

from __future__ import annotations

import math
from pathlib import Path
from typing import Iterator

import pandas as pd

from schema import Run, Sample


def _mannwhitneyu_greater(x: list[float], y: list[float]) -> tuple[float, float]:
    """One-sided Mann-Whitney U (alternative=greater), normal approximation.

    Returns (U, p_value). The normal approximation is calibrated for n ≥ 8 per
    group; for smaller n the p-value is approximate but the U statistic is
    exact. We use it because scipy is a 50MB dependency for one test.
    """
    nx, ny = len(x), len(y)
    if nx == 0 or ny == 0:
        return 0.0, 1.0
    pairs = [(v, 0) for v in x] + [(v, 1) for v in y]
    pairs.sort(key=lambda p: p[0])
    ranks = [0.0] * len(pairs)
    i = 0
    while i < len(pairs):
        j = i
        while j + 1 < len(pairs) and pairs[j + 1][0] == pairs[i][0]:
            j += 1
        avg_rank = (i + j) / 2 + 1
        for k in range(i, j + 1):
            ranks[k] = avg_rank
        i = j + 1
    R_x = sum(ranks[k] for k in range(len(pairs)) if pairs[k][1] == 0)
    U_x = R_x - nx * (nx + 1) / 2
    mean = nx * ny / 2
    sd = math.sqrt(nx * ny * (nx + ny + 1) / 12)
    if sd == 0:
        return U_x, 1.0
    z = (U_x - mean) / sd
    return U_x, 0.5 * math.erfc(z / math.sqrt(2))


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
    """Aggregate one run-id. Returns path to results.csv."""
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
                "dict_mode": run.dict_mode,
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
            "dict_mode": run.dict_mode,
            "seed": run.seed,
            "engine": run.engine,
            "duration_s": run.duration_s,
            "final_edges": run.final.edges,
            "final_execs": run.final.execs,
            "final_execs_per_sec": run.final.execs_per_sec,
            "final_crashes_unique": run.final.crashes_unique_engine,
            "time_to_first_crash_s": run.final.time_to_first_crash_s,
        })

    if not rows:
        raise RuntimeError(f"no cells found under {run_dir}")

    out_dir = reports_root / run_id
    out_dir.mkdir(parents=True, exist_ok=True)

    df = pd.DataFrame(rows)
    results_path = out_dir / "results.csv"
    df.to_csv(results_path, index=False)

    summary = pd.DataFrame(summary_rows)
    # Per-seed summary; report.py aggregates across seeds when rendering so
    # mean ± std + p-values come from the same source rows.
    summary.to_csv(out_dir / "summary.csv", index=False)

    # Mann-Whitney U vs raw, for each (sut, variant != raw) — skipped if
    # n_seeds < 3 since the test is meaningless on tiny samples.
    pvals: list[dict] = []
    for sut, sut_df in summary.groupby("sut"):
        raw_edges = list(sut_df[sut_df.variant == "raw"]["final_edges"].values)
        if len(raw_edges) < 3:
            continue
        for variant, var_df in sut_df.groupby("variant"):
            if variant == "raw":
                continue
            v_edges = list(var_df["final_edges"].values)
            if len(v_edges) < 3:
                continue
            U, p = _mannwhitneyu_greater(v_edges, raw_edges)
            pvals.append({"sut": sut, "variant": variant, "U": U, "p_vs_raw": p})
    if pvals:
        pd.DataFrame(pvals).to_csv(out_dir / "pvalues.csv", index=False)

    return results_path
