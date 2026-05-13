"""Assemble REPORT.md from results.csv + summary.csv + per-SUT plots."""

from __future__ import annotations

import datetime
from pathlib import Path

import pandas as pd
from jinja2 import Environment


def _md(df: pd.DataFrame, floatfmt: str = ".2f") -> str:
    """Render a small DataFrame as a GitHub-Flavoured Markdown table."""
    if df.empty:
        return "_(empty)_"
    def fmt(v):
        return format(v, floatfmt) if isinstance(v, float) else str(v)
    head = "| " + " | ".join(df.columns) + " |"
    sep  = "| " + " | ".join("---" for _ in df.columns) + " |"
    body = ["| " + " | ".join(fmt(v) for v in row) + " |"
            for row in df.itertuples(index=False)]
    return "\n".join([head, sep, *body])


REPORT_TMPL = """\
# Barkus Benchmark Report — `{{ run_id }}`

Generated {{ generated_at }} from
[`results.csv`](results.csv) and
[`summary.csv`](summary.csv).

## Provenance

| field | value |
|---|---|
{% for k, v in provenance.items() -%}
| {{ k }} | `{{ v }}` |
{% endfor %}

## Summary (per SUT × variant × dict_mode)

{{ summary_table }}

{% for sut in suts %}
## SUT: `{{ sut }}`

**Coverage over time** (mean ± std-dev band when seeds ≥ 2):

![{{ sut }} coverage](plots/{{ sut }}_coverage.png)

**Exec rate per variant**:

![{{ sut }} execs/sec](plots/{{ sut }}_eps.png)

**Crash count over time**:

![{{ sut }} crashes](plots/{{ sut }}_crashes.png)

{{ per_sut_tables[sut] }}

{% endfor %}

{% if pvalues_table %}
## Mann-Whitney U vs `raw` (one-sided, alternative=greater)

{{ pvalues_table }}

p-values < 0.05 indicate the variant's edges-at-end-of-run is
statistically greater than `raw`'s. Skipped on smoke runs (n_seeds < 3).
{% endif %}
"""


def render(report_dir: Path, run_id: str) -> Path:
    """Render REPORT.md inside report_dir. Returns its path."""
    summary = pd.read_csv(report_dir / "summary.csv")
    df = pd.read_csv(report_dir / "results.csv")

    env = Environment()
    tmpl = env.from_string(REPORT_TMPL)

    sample_row = df.iloc[0]
    provenance = {
        "run_id": run_id,
        "tier": int(sample_row["tier"]),
        "engine(s)": ", ".join(sorted(df["engine"].unique())),
        "barkus_sha": str(sample_row["barkus_sha"]),
        "n_cells": len(summary),
        "n_seeds_per_cell": int(summary["seed"].nunique()) if "seed" in summary else "?",
    }

    grouped = (
        summary.groupby(["sut", "variant", "dict_mode"])
        .agg(n_seeds=("seed", "count"),
             edges_mean=("final_edges", "mean"),
             edges_std=("final_edges", "std"),
             eps_mean=("final_execs_per_sec", "mean"),
             crashes=("final_crashes_unique", "sum"))
        .reset_index()
    )
    grouped["edges_std"] = grouped["edges_std"].fillna(0)
    summary_table = _md(grouped[["sut", "variant", "dict_mode", "n_seeds",
                                  "edges_mean", "edges_std",
                                  "eps_mean", "crashes"]])

    suts = sorted(grouped["sut"].unique())
    per_sut_tables: dict[str, str] = {}
    for sut in suts:
        s = grouped[grouped.sut == sut][
            ["variant", "dict_mode", "n_seeds", "edges_mean", "edges_std",
             "eps_mean", "crashes"]
        ]
        per_sut_tables[sut] = _md(s)

    pvalues_table = ""
    pv_path = report_dir / "pvalues.csv"
    if pv_path.exists():
        pv = pd.read_csv(pv_path)
        if not pv.empty:
            pvalues_table = _md(pv, floatfmt=".3f")

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
