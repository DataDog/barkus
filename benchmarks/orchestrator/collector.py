"""Cell collector: writes samples.jsonl + run.json for one cell.

M2 wired the `fake` engine for orchestrator self-test. M3 wires
`go-testing-f` (Go's native testing.F fuzzer) by running a pre-built
harness binary, parsing its stderr, and sampling at a fixed cadence.
M5/M6 will add libfuzzer + aflpp using the same shape.

Watchdog: every cell launches the harness in its own process group with
a daemon thread that SIGKILLs the whole group after duration_s + 30s.
Without it, harnesses with CGo (pg_query_go's libpg_query) or libFuzzer
fork-on-crash (html5ever) can run far past the engine's stated time
budget when the SUT hits an infinite loop on adversarial input. This
gives us cell isolation; the partial samples are still recorded.
"""

from __future__ import annotations

import json
import os
import platform
import shutil
import signal
import subprocess
import threading
import time
from pathlib import Path
from typing import Iterable

from parsers import go_native, libfuzzer
from parsers.fake import synthetic_sample
from schema import FinalStats, HostInfo, Run, Sample


def _host_info(cpu_pin: str | None = None) -> HostInfo:
    cpu = ""
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    except FileNotFoundError:
        cpu = platform.processor() or "unknown"
    return HostInfo(cpu=cpu or "unknown", kernel=platform.release(), cpu_pin=cpu_pin)


def write_samples(out_dir: Path, samples: Iterable[Sample]) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    with (out_dir / "samples.jsonl").open("w") as f:
        for s in samples:
            f.write(s.model_dump_json() + "\n")


def write_run(out_dir: Path, run: Run) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    # by_alias=True so the JSON has "dict" not "dict_mode" — matches the
    # output schema in the plan and what aggregate.py / report.py expect.
    (out_dir / "run.json").write_text(run.model_dump_json(indent=2, by_alias=True) + "\n")


def collect_fake(
    *,
    out_dir: Path,
    run_id: str,
    sut: str,
    variant: str,
    seed: int,
    dict_mode: str,
    duration_s: int,
    sample_period_s: int,
    barkus_sha: str,
    cpu_pin: int | None = None,
    tier: int = 0,
) -> Run:
    """Synthesize a full cell run for the fake engine and persist artifacts.

    The synthetic timeline uses 0..duration_s in steps of sample_period_s.
    Returns the Run object that was just written so callers can validate.
    """
    samples: list[Sample] = []
    for t in range(0, duration_s + 1, sample_period_s):
        raw = synthetic_sample(seed, t)
        samples.append(Sample(**raw))

    write_samples(out_dir, samples)

    last = samples[-1] if samples else None
    final = FinalStats(
        edges=last.edges if last else 0,
        execs=last.execs if last else 0,
        execs_per_sec=last.execs_per_sec if last else 0,
        crashes_unique_engine=last.crashes if last else 0,
    )
    run = Run(
        run_id=run_id,
        tier=tier,
        sut=sut,
        variant=variant,
        seed=seed,
        dict_mode=dict_mode,
        engine="fake",
        engine_version="m2-self-test",
        barkus_sha=barkus_sha,
        sut_sha="",
        grammar_path=None,
        grammar_sha=None,
        duration_s=duration_s,
        host=_host_info(cpu_pin=str(cpu_pin) if cpu_pin is not None else None),
        corpus_seeded=False,
        final=final,
    )
    write_run(out_dir, run)
    return run


def collect_go_native(
    *,
    out_dir: Path,
    run_id: str,
    sut: str,
    variant: str,
    seed: int,
    dict_mode: str,
    duration_s: int,
    sample_period_s: int,
    barkus_sha: str,
    sut_sha: str,
    grammar_path: str | None,
    binary_path: Path,
    fuzz_func: str,
    grammar_in_image: str | None,
    cpu_pin: int | None = None,
    tier: int = 1,
) -> Run:
    """Run a Go testing.F cell. Cold-start corpus, parse stderr, sample.

    The harness binary is launched with -test.fuzz=^<fuzz_func>$. We tail
    stderr, run each line through parsers.go_native.parse_line, and emit a
    Sample every sample_period_s seconds.
    """
    if not binary_path.exists():
        raise FileNotFoundError(
            f"go-testing-f harness not built: {binary_path} "
            f"(run benchmarks/suts/<sut>/build.sh first)"
        )

    corpus_dir = out_dir / "corpus"
    if corpus_dir.exists():
        shutil.rmtree(corpus_dir)
    corpus_dir.mkdir(parents=True)

    # Seed the env: BARKUS_JSON_GRAMMAR points the harness at the grammar
    # source file. On host (dev / smoke validation) we prefer the repo-
    # relative path; in the Docker child image, /opt/barkus/share/json.ebnf
    # is baked in by Dockerfile.base. Pick whichever exists.
    env = os.environ.copy()
    repo_root = Path(__file__).resolve().parents[2]
    candidates = []
    if grammar_path:
        candidates.append((repo_root / grammar_path).resolve())
    if grammar_in_image:
        candidates.append(Path(grammar_in_image))
    for candidate in candidates:
        if candidate.is_file():
            env["BARKUS_JSON_GRAMMAR"] = str(candidate)
            break
    else:
        if candidates:
            raise FileNotFoundError(
                f"grammar source not found at any of: {[str(c) for c in candidates]}"
            )

    # Wipe $GOCACHE/fuzz/<pkg> in addition to the per-cell -test.fuzzcachedir.
    # Go's fuzz harness reads "interesting" inputs from BOTH the cachedir AND
    # $GOCACHE/fuzz/<pkg>; without this wipe, "seeds" 1/2/3 would all start
    # from a shared corpus and the variance bands would be bogus.
    _wipe_go_fuzz_cache(binary_path)

    cmd_core = [
        str(binary_path),
        f"-test.fuzz=^{fuzz_func}$",
        f"-test.fuzztime={duration_s}s",
        # Kill the post-crash minimization loop (otherwise a single crash
        # can spend 60s+ minimizing).
        "-test.fuzzminimizetime=0s",
        # Hard wall-clock timeout — testing.F's `-fuzztime` is a soft
        # guidance and CGo-blocked harnesses (e.g. pg_query_go's
        # libpg_query infinite loops on adversarial input) can run far
        # past it. SIGKILL after duration + 30s buys cell isolation.
        f"-test.timeout={duration_s + 30}s",
        f"-test.fuzzcachedir={corpus_dir}",
        "-test.run=^$",
    ]
    cmd = _maybe_taskset(cpu_pin) + cmd_core
    # testing.F doesn't expose a -seed flag; per-cell corpus_dir + GOCACHE
    # wipe gives each (sut, variant, dict, seed) cell an independent
    # exploration trajectory. Document the seed in run.json so the report
    # carries the metadata even though the engine is seed-agnostic.
    _ = seed

    # Run the binary with cwd set to the per-cell directory. testing.F
    # persists minimized crash inputs to `testdata/fuzz/<Name>/` relative
    # to its CWD; without this isolation the orchestrator's directory
    # accumulates seed corpus across cells, which a) violates cold-start
    # and b) replays panics on next run (hit during M4 smoke). Each cell
    # gets its own testdata tree which is wiped along with corpus.
    testdata_dir = out_dir / "testdata"
    if testdata_dir.exists():
        shutil.rmtree(testdata_dir)

    # State carried across stderr lines. Go's testing.F emits running totals;
    # we just snapshot them at sample_period_s intervals.
    state: dict = {"execs": 0, "execs_per_sec": 0, "edges": 0, "crashes": 0, "rss_mb": 0}
    samples: list[Sample] = [Sample(t_s=0, **state)]

    # Make sure -test.fuzz prints to stderr in line-buffered mode.
    proc = subprocess.Popen(
        cmd,
        env=env,
        cwd=out_dir,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
        start_new_session=True,
    )
    _spawn_watchdog(proc, duration_s + 30)

    start = time.monotonic()
    next_sample_t = sample_period_s
    first_crash_t_s: int | None = None

    assert proc.stderr is not None
    for line in proc.stderr:
        # Update running state from the line if it parses.
        parsed = go_native.parse_line(line, state)
        if parsed is not None:
            state.update(parsed)

        # The fuzzer prints elapsed/execs lines AND any crash banners.
        # Crash detection is M3-light; M7 does the real dedup pass.
        if "FAIL" in line or "panic:" in line.lower():
            state["crashes"] = state.get("crashes", 0) + 1
            if first_crash_t_s is None:
                first_crash_t_s = int(time.monotonic() - start)

        # Emit samples at fixed cadence regardless of how often Go emits lines.
        elapsed = time.monotonic() - start
        while elapsed >= next_sample_t and next_sample_t <= duration_s:
            samples.append(Sample(t_s=next_sample_t, **state))
            next_sample_t += sample_period_s

    proc.wait(timeout=duration_s + 30)

    # Backfill any sample slots we missed (subprocess died early or stderr quiet).
    while next_sample_t <= duration_s:
        samples.append(Sample(t_s=next_sample_t, **state))
        next_sample_t += sample_period_s

    write_samples(out_dir, samples)

    last = samples[-1]
    final = FinalStats(
        edges=last.edges,
        execs=last.execs,
        execs_per_sec=last.execs_per_sec,
        crashes_unique_engine=last.crashes,
        time_to_first_crash_s=first_crash_t_s,
    )
    run = Run(
        run_id=run_id,
        tier=tier,
        sut=sut,
        variant=variant,
        seed=seed,
        dict_mode=dict_mode,
        engine="go-testing-f",
        engine_version=_go_version(),
        barkus_sha=barkus_sha,
        sut_sha=sut_sha,
        grammar_path=grammar_path,
        grammar_sha=None,
        duration_s=duration_s,
        host=_host_info(cpu_pin=str(cpu_pin) if cpu_pin is not None else None),
        corpus_seeded=False,
        final=final,
    )
    write_run(out_dir, run)
    return run


def _go_version() -> str:
    try:
        return subprocess.check_output(["go", "version"], text=True).strip()
    except Exception:
        return "unknown"


def _maybe_taskset(cpu_pin: int | None) -> list[str]:
    """Prefix returned by collectors so a cell pins to one CPU.

    Empty list when cpu_pin is None — useful for smoke runs and dev
    invocations where pinning isn't worth the noise.
    """
    if cpu_pin is None:
        return []
    return ["taskset", "-c", str(cpu_pin)]


def _wipe_go_fuzz_cache(binary_path: Path) -> None:
    """Wipe $GOCACHE/fuzz/<package> for the test binary about to run.

    Without this each cell inherits the previous cell's "interesting" inputs
    via Go's per-package fuzz cache, defeating cold-start. Best-effort: silent
    if GOCACHE isn't set or the path doesn't exist.
    """
    gocache = os.environ.get("GOCACHE")
    if not gocache:
        try:
            gocache = subprocess.check_output(
                ["go", "env", "GOCACHE"], text=True
            ).strip()
        except Exception:
            return
    fuzz_root = Path(gocache) / "fuzz"
    if not fuzz_root.exists():
        return
    pkg_name = binary_path.stem
    for child in fuzz_root.iterdir():
        if child.is_dir() and (child.name == pkg_name or pkg_name in child.name):
            try:
                shutil.rmtree(child)
            except OSError:
                pass


def _spawn_watchdog(proc: subprocess.Popen, timeout_s: int) -> threading.Thread:
    """Daemon that SIGKILLs proc's process group if it outlives timeout_s.

    Used by every real-engine collector. Without it, harnesses that
    deadlock inside the SUT (CGo, libFuzzer fork-on-crash) hang
    indefinitely even with the engine's own time-limit flag.

    SIGKILL alone is not enough: orphaned workers in the killed pgroup
    can keep stderr's write-end open, so the parent's `for line in
    proc.stderr` loop blocks forever waiting for a new line. We also
    explicitly close stderr so the iterator returns cleanly.
    """
    def _kill():
        time.sleep(timeout_s)
        if proc.poll() is None:
            try:
                os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass
            try:
                if proc.stderr is not None:
                    proc.stderr.close()
            except Exception:
                pass
    t = threading.Thread(target=_kill, daemon=True)
    t.start()
    return t


def collect_libfuzzer(
    *,
    out_dir: Path,
    run_id: str,
    sut: str,
    variant: str,
    seed: int,
    dict_mode: str,
    duration_s: int,
    sample_period_s: int,
    barkus_sha: str,
    sut_sha: str,
    grammar_path: str | None,
    binary_path: Path,
    cpu_pin: int | None = None,
    tier: int = 1,
) -> Run:
    """Run a libFuzzer (cargo-fuzz) cell. Cold-start corpus, parse stderr."""
    if not binary_path.exists():
        raise FileNotFoundError(
            f"libfuzzer harness not built: {binary_path} "
            f"(run benchmarks/suts/<sut>/build.sh first)"
        )

    corpus_dir = out_dir / "corpus"
    if corpus_dir.exists():
        shutil.rmtree(corpus_dir)
    corpus_dir.mkdir(parents=True)

    cmd_core = [
        str(binary_path),
        f"-max_total_time={duration_s}",
        # Per-cell seed: makes seeds 1/2/3 produce genuinely independent
        # exploration trajectories. Without this, libFuzzer's PRNG would
        # default to a wall-clock-derived seed and the variance bands would
        # reflect run-time jitter rather than seed variance.
        f"-seed={seed}",
        "-print_final_stats=1",
        "-close_fd_mask=3",
        str(corpus_dir),
    ]
    cmd = _maybe_taskset(cpu_pin) + cmd_core
    _ = dict_mode  # libFuzzer auto-extracts a dict from the binary; no flag toggles it

    state: dict = {"execs": 0, "execs_per_sec": 0, "edges": 0, "crashes": 0, "rss_mb": 0}
    samples: list[Sample] = [Sample(t_s=0, **state)]

    proc = subprocess.Popen(
        cmd,
        cwd=out_dir,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
        start_new_session=True,
    )
    _spawn_watchdog(proc, duration_s + 30)

    start = time.monotonic()
    next_sample_t = sample_period_s
    first_crash_t_s: int | None = None
    assert proc.stderr is not None
    for line in proc.stderr:
        parsed = libfuzzer.parse_line(line, state)
        if parsed is not None:
            state.update(parsed)

        if "ERROR:" in line and "Sanitizer" in line:
            state["crashes"] = state.get("crashes", 0) + 1
            if first_crash_t_s is None:
                first_crash_t_s = int(time.monotonic() - start)

        elapsed = time.monotonic() - start
        while elapsed >= next_sample_t and next_sample_t <= duration_s:
            samples.append(Sample(t_s=next_sample_t, **state))
            next_sample_t += sample_period_s

    proc.wait(timeout=duration_s + 30)
    while next_sample_t <= duration_s:
        samples.append(Sample(t_s=next_sample_t, **state))
        next_sample_t += sample_period_s

    write_samples(out_dir, samples)
    last = samples[-1]
    final = FinalStats(
        edges=last.edges,
        execs=last.execs,
        execs_per_sec=last.execs_per_sec,
        crashes_unique_engine=last.crashes,
        time_to_first_crash_s=first_crash_t_s,
    )
    run = Run(
        run_id=run_id, tier=tier, sut=sut, variant=variant, seed=seed,
        dict_mode=dict_mode, engine="libfuzzer",
        engine_version="cargo-fuzz 0.12.0 + libfuzzer-sys 0.4.10",
        barkus_sha=barkus_sha, sut_sha=sut_sha,
        grammar_path=grammar_path, grammar_sha=None,
        duration_s=duration_s,
        host=_host_info(cpu_pin=str(cpu_pin) if cpu_pin is not None else None),
        corpus_seeded=False, final=final,
    )
    write_run(out_dir, run)
    return run


def validate_cell(out_dir: Path) -> tuple[bool, str]:
    """Re-read the artifacts written by collect_fake and validate them.

    Used by `run.py smoke` as the M2 done-when check: artifacts must be
    schema-valid AND the time series must be non-empty + monotonic in execs.
    """
    run_path = out_dir / "run.json"
    samples_path = out_dir / "samples.jsonl"
    if not run_path.exists():
        return False, f"missing {run_path}"
    if not samples_path.exists():
        return False, f"missing {samples_path}"

    try:
        Run.model_validate_json(run_path.read_text())
    except Exception as e:  # pragma: no cover - validation error path
        return False, f"run.json invalid: {e}"

    samples: list[Sample] = []
    for line in samples_path.read_text().splitlines():
        if not line.strip():
            continue
        try:
            samples.append(Sample.model_validate_json(line))
        except Exception as e:  # pragma: no cover
            return False, f"samples.jsonl invalid: {e}"
    if not samples:
        return False, "samples.jsonl is empty"
    for prev, cur in zip(samples, samples[1:]):
        if cur.execs < prev.execs:
            return False, f"execs went backward at t_s={cur.t_s}"

    run = Run.model_validate_json(run_path.read_text())
    # The fake engine deliberately reports zero execs at t=0; for any real
    # engine, at least one sample must have nonzero execs (otherwise the
    # subprocess silently failed — the M3 raw cell's first run hit this).
    if run.engine != "fake":
        if not any(s.execs > 0 for s in samples):
            return False, "no sample has execs > 0 — subprocess produced no fuzz output"

    return True, f"{len(samples)} samples, final edges={samples[-1].edges}"
