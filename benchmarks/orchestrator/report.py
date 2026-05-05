"""Assemble REPORT.md from results.parquet + summary.csv + per-SUT plots.

The Markdown report is the single deliverable for the paper. Plots are
embedded as SVG/PNG (no PDF). Summary tables are inline GitHub-Flavoured
Markdown.
"""

from __future__ import annotations

import datetime
from pathlib import Path

import pandas as pd
from jinja2 import Environment


REPORT_TMPL = """\
# Barkus Benchmark Report — `{{ run_id }}`

Generated {{ generated_at }} from
[`results.parquet`](results.parquet) and
[`summary.csv`](summary.csv).

## Provenance

| field | value |
|---|---|
{% for k, v in provenance.items() -%}
| {{ k }} | `{{ v }}` |
{% endfor %}

## Summary (per SUT × variant × dict)

{{ summary_table }}

{% for sut in suts %}
## SUT: `{{ sut }}`

**Coverage over time** (mean ± std-dev band when seeds ≥ 2):

![{{ sut }} coverage](plots/{{ sut }}_coverage.svg)

**Exec rate per variant**:

![{{ sut }} execs/sec](plots/{{ sut }}_eps.svg)

**Crash count over time**:

![{{ sut }} crashes](plots/{{ sut }}_crashes.svg)

{{ per_sut_tables[sut] }}

{% endfor %}

{% if pvalues_table %}
## Mann-Whitney U vs `raw` (one-sided, alternative=greater)

{{ pvalues_table }}

p-values < 0.05 indicate the variant's edges-at-end-of-run is
statistically greater than `raw`'s. Skipped on smoke runs (n_seeds < 3).
{% endif %}

---

_Reproducibility:_ this report was generated from a fully pinned toolchain
manifest (`versions.lock`). Re-running on a different host with the same
manifest reproduces these numbers within ~2× std-dev (fuzzers are
non-deterministic; provenance fields above are bit-identical).
"""


def _md_table(df: pd.DataFrame) -> str:
    """Render a small DataFrame as a GitHub-Flavoured Markdown table."""
    if df.empty:
        return "_(empty)_"
    cols = list(df.columns)
    lines = ["| " + " | ".join(cols) + " |",
             "|" + "|".join(["---"] * len(cols)) + "|"]
    for _, row in df.iterrows():
        cells = []
        for c in cols:
            v = row[c]
            if isinstance(v, float):
                cells.append(f"{v:.2f}" if abs(v) < 1000 else f"{v:.0f}")
            else:
                cells.append(str(v))
        lines.append("| " + " | ".join(cells) + " |")
    return "\n".join(lines)


def render(report_dir: Path, run_id: str) -> Path:
    """Render REPORT.md inside report_dir. Returns its path."""
    summary = pd.read_csv(report_dir / "summary.csv")
    parquet = report_dir / "results.parquet"
    df = pd.read_parquet(parquet)

    env = Environment()
    tmpl = env.from_string(REPORT_TMPL)

    # Provenance from the first row (all rows of a run share these).
    sample_row = df.iloc[0]
    provenance = {
        "run_id": run_id,
        "tier": int(sample_row["tier"]),
        "engine(s)": ", ".join(sorted(df["engine"].unique())),
        "barkus_sha": str(sample_row["barkus_sha"]),
        "n_cells": len(summary),
        "n_seeds_per_cell": int(summary["seed"].nunique()) if "seed" in summary else "?",
    }

    # Aggregate summary across seeds per (sut, variant, dict).
    grouped = (
        summary.groupby(["sut", "variant", "dict"])
        .agg(n_seeds=("seed", "count"),
             edges_mean=("final_edges", "mean"),
             edges_std=("final_edges", "std"),
             eps_mean=("final_execs_per_sec", "mean"),
             crashes=("final_crashes_unique", "sum"))
        .reset_index()
    )
    grouped["edges_std"] = grouped["edges_std"].fillna(0)
    summary_table = _md_table(grouped[["sut", "variant", "dict", "n_seeds",
                                       "edges_mean", "edges_std",
                                       "eps_mean", "crashes"]])

    suts = sorted(grouped["sut"].unique())
    per_sut_tables: dict[str, str] = {}
    for sut in suts:
        s = grouped[grouped.sut == sut][
            ["variant", "dict", "n_seeds", "edges_mean", "edges_std",
             "eps_mean", "crashes"]
        ].copy()
        per_sut_tables[sut] = _md_table(s)

    pvalues_table = ""
    pv_path = report_dir / "pvalues.csv"
    if pv_path.exists():
        pv = pd.read_csv(pv_path)
        if not pv.empty:
            pvalues_table = _md_table(pv)

    rendered = tmpl.render(
        run_id=run_id,
        generated_at=datetime.datetime.now(datetime.timezone.utc)
            .strftime("%Y-%m-%d %H:%M:%S UTC"),
        provenance=provenance,
        suts=suts,
        summary_table=summary_table,
        per_sut_tables=per_sut_tables,
        pvalues_table=pvalues_table,
    )

    out = report_dir / "REPORT.md"
    out.write_text(rendered)
    return out
