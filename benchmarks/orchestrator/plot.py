"""Generate per-SUT plots (PNG) from results.csv."""

from __future__ import annotations

from pathlib import Path

import matplotlib

matplotlib.use("Agg")  # headless; no DISPLAY needed
import matplotlib.pyplot as plt  # noqa: E402  (must follow the use() call)
import pandas as pd  # noqa: E402


def _save(fig, out_dir: Path, name: str) -> Path:
    path = out_dir / f"{name}.png"
    fig.savefig(path, format="png", bbox_inches="tight", dpi=144)
    plt.close(fig)
    return path


def plot_coverage_over_time(df: pd.DataFrame, sut: str, out_dir: Path) -> Path:
    """One line per (variant, dict_mode). Mean ± std band when seeds >= 2."""
    sub = df[df.sut == sut]
    fig, ax = plt.subplots(figsize=(9, 5))
    for (variant, dict_mode), g in sub.groupby(["variant", "dict_mode"]):
        agg = g.groupby("t_s")["edges"].agg(["mean", "std", "count"]).reset_index()
        label = f"{variant} (dict={dict_mode})"
        line, = ax.plot(agg.t_s, agg["mean"], label=label, linewidth=1.6)
        if (agg["count"] > 1).any():
            ax.fill_between(
                agg.t_s,
                agg["mean"] - agg["std"].fillna(0),
                agg["mean"] + agg["std"].fillna(0),
                alpha=0.15,
                color=line.get_color(),
            )
    ax.set_xlabel("elapsed (s)")
    ax.set_ylabel("edges discovered")
    ax.set_title(f"{sut} — coverage over time")
    ax.grid(True, alpha=0.3)
    ax.legend(loc="lower right", fontsize=8, ncol=2)
    return _save(fig, out_dir, f"{sut}_coverage")


def plot_execs_per_sec(df: pd.DataFrame, sut: str, out_dir: Path) -> Path:
    """Bar chart of mean execs/sec per (variant, dict_mode)."""
    sub = df[df.sut == sut]
    summary = (
        sub.groupby(["variant", "dict_mode"])["execs_per_sec"]
        .mean()
        .reset_index()
        .pivot(index="variant", columns="dict_mode", values="execs_per_sec")
    )
    fig, ax = plt.subplots(figsize=(8, 4))
    summary.plot(kind="bar", ax=ax, edgecolor="black")
    ax.set_ylabel("execs/sec (mean over run)")
    ax.set_title(f"{sut} — exec rate per variant")
    ax.set_xlabel("")
    ax.grid(True, axis="y", alpha=0.3)
    plt.xticks(rotation=20, ha="right")
    return _save(fig, out_dir, f"{sut}_eps")


def plot_crashes_over_time(df: pd.DataFrame, sut: str, out_dir: Path) -> Path:
    """Step-plot of unique-engine crashes per (variant, dict_mode)."""
    sub = df[df.sut == sut]
    fig, ax = plt.subplots(figsize=(9, 4))
    for (variant, dict_mode), g in sub.groupby(["variant", "dict_mode"]):
        agg = g.groupby("t_s")["crashes"].mean().reset_index()
        ax.step(
            agg.t_s, agg["crashes"], label=f"{variant} (dict={dict_mode})",
            where="post", linewidth=1.4,
        )
    ax.set_xlabel("elapsed (s)")
    ax.set_ylabel("unique crashes (engine)")
    ax.set_title(f"{sut} — crashes over time")
    ax.grid(True, alpha=0.3)
    ax.legend(loc="upper left", fontsize=8, ncol=2)
    return _save(fig, out_dir, f"{sut}_crashes")


def plot_all(results_path: Path, out_dir: Path) -> dict[str, dict[str, Path]]:
    """For each SUT in results.csv, emit coverage / eps / crashes plots."""
    df = pd.read_csv(results_path)
    plots: dict[str, dict[str, Path]] = {}
    for sut in sorted(df["sut"].unique()):
        plots[sut] = {
            "coverage": plot_coverage_over_time(df, sut, out_dir),
            "eps":      plot_execs_per_sec(df, sut, out_dir),
            "crashes":  plot_crashes_over_time(df, sut, out_dir),
        }
    return plots
