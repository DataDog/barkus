#!/usr/bin/env python3
"""Barkus benchmark orchestrator entrypoint.

Subcommands:

    build      run every benchmarks/suts/<id>/build.sh in dependency order
    smoke      60s/cell gate; one seed; serial; surfaces broken harnesses
    full       4h × N seeds × 2 dict-modes per cell; ThreadPoolExecutor with
               taskset CPU pinning.
    aggregate  samples.jsonl × all cells -> results.csv + summary.csv
    report     plots (PNG) + REPORT.md

Exit code from smoke/full = number of failed cells.
"""

from __future__ import annotations

import argparse
import datetime
import queue
import shutil
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

import yaml  # type: ignore[import-untyped]

from collector import (
    collect_aflpp,
    collect_fake,
    collect_go_native,
    collect_libfuzzer,
    validate_cell,
)


REPO_ROOT = Path(__file__).resolve().parents[2]
BENCH_ROOT = REPO_ROOT / "benchmarks"
SUTS_ROOT = BENCH_ROOT / "suts"
RESULTS_ROOT = BENCH_ROOT / "results"
REPORTS_ROOT = BENCH_ROOT / "reports"


# Build order matters only insofar as the FFI must be compiled before any
# harness that links against it. `cargo build -p barkus-ffi --release`
# is invoked transitively by every Go SUT's build.sh (and via path = "..."
# for Rust SUTs), so the per-SUT order itself doesn't matter — alphabetical
# is fine.
SUTS_BUILD_ORDER = [
    "simdjson-go",
    "vitess",
    "yaml-go",
    "pg_query_go",
    "rust-cssparser",
    "resvg",
    "html5ever",
    "libxml2",
]


def _now_run_id(stage: str) -> str:
    return datetime.datetime.now(datetime.timezone.utc).strftime(
        "%Y-%m-%dT%H%M%SZ"
    ) + f"_{stage}"


def _barkus_sha() -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=REPO_ROOT, text=True,
        ).strip()
    except Exception:
        return "unknown"


def _load_config(path: Path) -> dict:
    return yaml.safe_load(path.read_text())


def _enumerate_cells(
    cfg: dict, tier: set[int], seeds: list[int], sut_filter: str | None = None,
) -> list[dict]:
    cells: list[dict] = []
    dict_modes = cfg["run"]["dict_modes"]
    for sut in cfg["suts"]:
        if sut.get("tier", 1) not in tier:
            continue
        if sut_filter and sut["id"] != sut_filter:
            continue
        for variant in sut["variants"]:
            for dm in dict_modes:
                for seed in seeds:
                    cells.append({
                        "sut": sut["id"],
                        "sut_obj": sut,
                        "engine": sut["engine"],
                        "variant": variant["id"],
                        "variant_obj": variant,
                        "dict": dm,
                        "seed": seed,
                        "tier": sut.get("tier", 1),
                    })
    return cells


# -- Cell runner (shared by smoke + full) ------------------------------------

def _run_cell(
    cell: dict,
    *,
    run_id: str,
    stage: str,
    duration_s: int,
    sample_period_s: int,
    barkus_sha: str,
    cpu_pin: int | None,
) -> tuple[bool, str, Path]:
    """Invoke the right collect_* for this cell. Returns (ok, msg, cell_dir)."""
    cell_dir = (
        RESULTS_ROOT / stage / run_id / cell["sut"] / cell["variant"]
        / f"dict-{cell['dict']}" / f"seed-{cell['seed']}"
    )
    if cell_dir.exists():
        shutil.rmtree(cell_dir)
    cell_dir.mkdir(parents=True)

    engine = cell["engine"]
    sut_obj = cell["sut_obj"]
    variant_obj = cell["variant_obj"]

    try:
        if engine == "fake":
            collect_fake(
                out_dir=cell_dir, run_id=run_id, sut=cell["sut"],
                variant=cell["variant"], seed=cell["seed"], dict_mode=cell["dict"],
                duration_s=duration_s, sample_period_s=sample_period_s,
                barkus_sha=barkus_sha, cpu_pin=cpu_pin, tier=cell["tier"],
            )
        elif engine == "go-testing-f":
            binary_rel = sut_obj.get("harness_binary")
            if not binary_rel:
                return False, "missing harness_binary in config.yaml", cell_dir
            collect_go_native(
                out_dir=cell_dir, run_id=run_id, sut=cell["sut"],
                variant=cell["variant"], seed=cell["seed"], dict_mode=cell["dict"],
                duration_s=duration_s, sample_period_s=sample_period_s,
                barkus_sha=barkus_sha,
                sut_sha=sut_obj.get("pin", {}).get("commit", ""),
                grammar_path=sut_obj.get("grammar"),
                binary_path=REPO_ROOT / binary_rel,
                fuzz_func=variant_obj["fuzz_func"],
                grammar_in_image=sut_obj.get("grammar_path_in_image"),
                cpu_pin=cpu_pin, tier=cell["tier"],
            )
        elif engine in ("libfuzzer", "aflpp"):
            harness_dir = sut_obj.get("harness_dir")
            if not harness_dir:
                return False, f"missing harness_dir in config.yaml", cell_dir
            binary_path = REPO_ROOT / harness_dir / variant_obj["id"]
            collector = collect_libfuzzer if engine == "libfuzzer" else collect_aflpp
            collector(
                out_dir=cell_dir, run_id=run_id, sut=cell["sut"],
                variant=cell["variant"], seed=cell["seed"], dict_mode=cell["dict"],
                duration_s=duration_s, sample_period_s=sample_period_s,
                barkus_sha=barkus_sha,
                sut_sha=sut_obj.get("pin", {}).get("commit", ""),
                grammar_path=sut_obj.get("grammar"),
                binary_path=binary_path,
                cpu_pin=cpu_pin, tier=cell["tier"],
            )
        else:
            return False, f"engine '{engine}' not wired", cell_dir
    except Exception as e:
        return False, str(e), cell_dir

    return (*validate_cell(cell_dir), cell_dir)


def _label(cell: dict) -> str:
    return (
        f"{cell['sut']}/{cell['variant']} "
        f"dict={cell['dict']} seed={cell['seed']}"
    )


# -- subcommands -------------------------------------------------------------

def cmd_smoke(args: argparse.Namespace) -> int:
    cfg = _load_config(args.config)
    tiers = {int(t) for t in args.tier.split(",")}
    seeds = [int(args.seed)] if args.seed is not None else cfg["run"]["seeds"][:1]

    cells = _enumerate_cells(cfg, tiers, seeds, sut_filter=args.sut)
    if not cells:
        print(f"smoke: no cells matched tier={tiers} sut={args.sut!r}", file=sys.stderr)
        return 1

    run_id = _now_run_id("smoke")
    barkus_sha = _barkus_sha()
    duration = cfg["run"]["smoke_duration_s"]
    period = cfg["defaults"]["sample_period_s"]
    failures: list[str] = []

    for cell in cells:
        ok, msg, _ = _run_cell(
            cell,
            run_id=run_id, stage="smoke",
            duration_s=duration, sample_period_s=period,
            barkus_sha=barkus_sha, cpu_pin=None,
        )
        tag = "OK" if ok else "FAIL"
        print(f"  smoke[{tag}] {_label(cell)}: {msg}")
        if not ok:
            failures.append(f"{_label(cell)}: {msg}")

    if failures:
        print(f"\nsmoke: {len(failures)} failure(s):", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return len(failures)
    print(f"\nsmoke: {len(cells)} cell(s) green ({run_id})")
    return 0


def cmd_full(args: argparse.Namespace) -> int:
    cfg = _load_config(args.config)
    tiers = {int(t) for t in args.tier.split(",")}
    seeds = cfg["run"]["seeds"]
    if args.seeds is not None:
        seeds = [int(s) for s in args.seeds.split(",")]

    cells = _enumerate_cells(cfg, tiers, seeds, sut_filter=args.sut)
    if not cells:
        print(f"full: no cells matched tier={tiers}", file=sys.stderr)
        return 1

    run_id = _now_run_id("full")
    barkus_sha = _barkus_sha()
    duration = cfg["run"]["full_duration_s"]
    period = cfg["defaults"]["sample_period_s"]
    parallel = cfg["run"]["parallel_cells"]
    if args.parallel is not None:
        parallel = int(args.parallel)

    print(f"full: run_id={run_id} cells={len(cells)} "
          f"duration={duration}s parallel={parallel}")
    eta_s = (len(cells) * duration) / max(parallel, 1)
    print(f"  estimated wall-clock: {eta_s/3600:.1f} h "
          f"({datetime.timedelta(seconds=int(eta_s))})")

    # CPU pin allocator: each worker pops a slot, runs the cell, returns
    # the slot. ThreadPoolExecutor bounds concurrency to `parallel`, so
    # the queue is sized to match and never blocks.
    slots: queue.Queue[int] = queue.Queue()
    for i in range(parallel):
        slots.put(i)

    failures: list[str] = []
    completed = 0
    started_at = time.monotonic()

    def _worker(cell: dict) -> tuple[dict, bool, str]:
        slot = slots.get()
        try:
            ok, msg, _ = _run_cell(
                cell,
                run_id=run_id, stage="full",
                duration_s=duration, sample_period_s=period,
                barkus_sha=barkus_sha, cpu_pin=slot,
            )
            return cell, ok, msg
        finally:
            slots.put(slot)

    with ThreadPoolExecutor(max_workers=parallel) as ex:
        futures = [ex.submit(_worker, c) for c in cells]
        for fut in as_completed(futures):
            cell, ok, msg = fut.result()
            completed += 1
            tag = "OK" if ok else "FAIL"
            elapsed = time.monotonic() - started_at
            remaining = max(0, eta_s - elapsed)
            print(
                f"  [{completed}/{len(cells)}] full[{tag}] {_label(cell)}: "
                f"{msg} (eta {datetime.timedelta(seconds=int(remaining))})"
            )
            if not ok:
                failures.append(f"{_label(cell)}: {msg}")

    if failures:
        print(f"\nfull: {len(failures)} failure(s) of {len(cells)}", file=sys.stderr)
        return len(failures)
    print(f"\nfull: {len(cells)} cell(s) green ({run_id})")
    print(f"  next: python run.py aggregate --run-id {run_id}")
    print(f"        python run.py report    --run-id {run_id}")
    return 0


def cmd_build(args: argparse.Namespace) -> int:
    """Run every suts/<id>/build.sh in order. Idempotent."""
    failures: list[str] = []
    suts = args.suts.split(",") if args.suts else SUTS_BUILD_ORDER
    for sut_id in suts:
        script = SUTS_ROOT / sut_id / "build.sh"
        if not script.exists():
            print(f"  build[SKIP] {sut_id} (no build.sh)")
            continue
        print(f"  build[RUN]  {sut_id}")
        rc = subprocess.run(
            ["bash", str(script)], cwd=script.parent
        ).returncode
        if rc != 0:
            print(f"  build[FAIL] {sut_id} (rc={rc})", file=sys.stderr)
            failures.append(sut_id)
        else:
            print(f"  build[OK]   {sut_id}")
    if failures:
        return len(failures)
    print(f"\nbuild: {len(suts)} SUT(s) built")
    return 0


def cmd_aggregate(args: argparse.Namespace) -> int:
    from aggregate import aggregate
    out = aggregate(args.run_id, RESULTS_ROOT, REPORTS_ROOT)
    print(f"aggregate: wrote {out}")
    return 0


def cmd_report(args: argparse.Namespace) -> int:
    from plot import plot_all
    from report import render
    report_dir = REPORTS_ROOT / args.run_id
    if not report_dir.exists():
        print(f"report: {report_dir} not found — run aggregate first", file=sys.stderr)
        return 1
    plots_dir = report_dir / "plots"
    plots_dir.mkdir(exist_ok=True)
    results_csv = report_dir / "results.csv"
    plot_all(results_csv, plots_dir)
    out = render(report_dir, args.run_id)
    print(f"report: wrote {out}")
    return 0


# -- argparse ---------------------------------------------------------------

def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Barkus benchmark orchestrator")
    parser.add_argument("--config", type=Path,
                        default=Path(__file__).parent / "config.yaml")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_build = sub.add_parser("build", help="run every suts/<id>/build.sh")
    p_build.add_argument("--suts", default=None,
                         help="comma-separated SUT ids (default: all)")
    p_build.set_defaults(func=cmd_build)

    p_smoke = sub.add_parser("smoke", help="60s/cell smoke gate")
    p_smoke.add_argument("--tier", default="0,1,2")
    p_smoke.add_argument("--seed", default=None)
    p_smoke.add_argument("--sut", default=None)
    p_smoke.set_defaults(func=cmd_smoke)

    p_full = sub.add_parser("full", help="full 4h × seeds × dict run")
    p_full.add_argument("--tier", default="1",
                        help="comma-separated tier list (default: 1)")
    p_full.add_argument("--seeds", default=None,
                        help="comma-separated seed override (default: cfg.run.seeds)")
    p_full.add_argument("--sut", default=None,
                        help="restrict to a single SUT id")
    p_full.add_argument("--parallel", default=None,
                        help="parallel cells override (default: cfg.run.parallel_cells)")
    p_full.set_defaults(func=cmd_full)

    p_agg = sub.add_parser("aggregate", help="samples.jsonl -> results.csv")
    p_agg.add_argument("--run-id", required=True)
    p_agg.set_defaults(func=cmd_aggregate)

    p_rep = sub.add_parser("report", help="plots + REPORT.md")
    p_rep.add_argument("--run-id", required=True)
    p_rep.set_defaults(func=cmd_report)

    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
