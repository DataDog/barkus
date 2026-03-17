use crate::json;
use crate::reachability::ReachabilityReport;
use crate::recommend::Recommendation;
use crate::stats::CorpusStats;

pub fn render(stats: &CorpusStats, reachability: &ReachabilityReport, recommendations: &[Recommendation], grammar_path: &str) -> String {
    let json = json::render_compact(stats, reachability, recommendations, grammar_path);
    let escaped_path = html_escape(grammar_path);

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>barkus-viz — {escaped_path}</title>
<style>
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0d1117; color: #c9d1d9; padding: 24px; }}
h1 {{ font-size: 1.5rem; margin-bottom: 8px; color: #58a6ff; }}
h2 {{ font-size: 1.2rem; margin: 24px 0 12px; color: #58a6ff; border-bottom: 1px solid #21262d; padding-bottom: 4px; }}
h3 {{ font-size: 1rem; margin: 16px 0 8px; color: #8b949e; }}

.banner {{ background: #161b22; border: 1px solid #30363d; border-radius: 6px; padding: 16px 24px; margin-bottom: 24px; display: flex; gap: 32px; flex-wrap: wrap; }}
.banner .stat {{ text-align: center; }}
.banner .stat .value {{ font-size: 2rem; font-weight: bold; color: #f0f6fc; }}
.banner .stat .label {{ font-size: 0.85rem; color: #8b949e; }}

.chart-container {{ background: #161b22; border: 1px solid #30363d; border-radius: 6px; padding: 16px; margin-bottom: 16px; }}
.bar-row {{ display: flex; align-items: center; margin: 3px 0; font-size: 0.8rem; }}
.bar-label {{ width: 100px; text-align: right; padding-right: 8px; color: #8b949e; flex-shrink: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}
.bar {{ height: 18px; border-radius: 3px; min-width: 1px; transition: width 0.2s; }}
.bar-value {{ padding-left: 6px; color: #8b949e; font-size: 0.75rem; }}

table {{ width: 100%; border-collapse: collapse; font-size: 0.85rem; }}
th, td {{ padding: 6px 10px; text-align: left; border-bottom: 1px solid #21262d; }}
th {{ cursor: pointer; color: #58a6ff; user-select: none; position: sticky; top: 0; background: #161b22; }}
th:hover {{ color: #79c0ff; }}
tr:hover {{ background: #1c2128; }}
.table-wrapper {{ background: #161b22; border: 1px solid #30363d; border-radius: 6px; padding: 16px; margin-bottom: 16px; max-height: 600px; overflow-y: auto; }}

.alt-bar {{ display: inline-block; height: 14px; border-radius: 2px; vertical-align: middle; }}
.alt-breakdown {{ display: none; padding: 8px 0 4px 20px; }}
.expandable {{ cursor: pointer; }}
.expandable:before {{ content: '\25B6'; margin-right: 6px; font-size: 0.7rem; }}
.expandable.open:before {{ content: '\25BC'; }}

.treemap {{ position: relative; width: 100%; height: 400px; background: #161b22; border: 1px solid #30363d; border-radius: 6px; overflow: hidden; }}
.treemap-cell {{ position: absolute; border: 1px solid #0d1117; display: flex; align-items: center; justify-content: center; font-size: 0.7rem; overflow: hidden; cursor: pointer; transition: opacity 0.15s; }}
.treemap-cell:hover {{ opacity: 0.85; z-index: 1; outline: 2px solid #58a6ff; }}
.treemap-tooltip {{ position: fixed; background: #1c2128; border: 1px solid #30363d; border-radius: 4px; padding: 8px 12px; font-size: 0.8rem; pointer-events: none; z-index: 100; display: none; }}

.issue-list {{ list-style: none; }}
.issue-list li {{ background: #161b22; border: 1px solid #30363d; border-radius: 6px; padding: 10px 14px; margin-bottom: 8px; }}
.issue-list .tag {{ display: inline-block; font-size: 0.7rem; padding: 2px 6px; border-radius: 3px; margin-right: 6px; font-weight: 600; }}
.tag-unreached {{ background: #da3633; color: #fff; }}
.tag-low {{ background: #d29922; color: #000; }}
.tag-starved {{ background: #bc8cff; color: #000; }}
.tag-choke {{ background: #388bfd; color: #fff; }}
.tag-weight {{ background: #8b949e; color: #000; }}
.explanation {{ color: #8b949e; font-size: 0.8rem; margin-top: 4px; }}
</style>
</head>
<body>

<h1>barkus-viz Coverage Report</h1>
<p style="color:#8b949e;margin-bottom:16px;font-size:0.9rem">Grammar: <code style="color:#c9d1d9">{escaped_path}</code></p>

<div id="banner" class="banner"></div>

<div id="recommendations"></div>

<h2>Depth Distribution</h2>
<div id="depth-chart" class="chart-container"></div>

<h2>Node Count Distribution</h2>
<div id="node-chart" class="chart-container"></div>

<h2>Production Coverage</h2>
<div id="prod-table" class="table-wrapper"></div>

<h2>Grammar Treemap</h2>
<div id="treemap" class="treemap"></div>
<div id="treemap-tooltip" class="treemap-tooltip"></div>

<h2>Hard-to-Reach Analysis</h2>
<div id="issues"></div>

<script>
const DATA = {json};

(function() {{
    const S = DATA.stats;
    const R = DATA.reachability;

    function esc(s) {{ return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;'); }}

    // --- Summary Banner ---
    const coveredCount = S.productions.filter(p => p.payload_hit_count > 0).length;
    const totalProds = S.productions.length;
    const coveragePct = totalProds > 0 ? (coveredCount / totalProds * 100).toFixed(1) : '0.0';
    const failRate = S.total_payloads > 0 ? (S.failures / S.total_payloads * 100).toFixed(2) : '0.00';

    document.getElementById('banner').innerHTML = `
        <div class="stat"><div class="value">${{S.total_payloads.toLocaleString()}}</div><div class="label">Payloads</div></div>
        <div class="stat"><div class="value">${{failRate}}%</div><div class="label">Failure Rate</div><div style="font-size:0.75rem;color:#8b949e;margin-top:4px">${{S.failure_breakdown.max_depth.toLocaleString()}} max depth · ${{S.failure_breakdown.max_total_nodes.toLocaleString()}} max nodes</div></div>
        <div class="stat"><div class="value">${{coveredCount}} / ${{totalProds}}</div><div class="label">Productions Hit</div></div>
        <div class="stat"><div class="value">${{coveragePct}}%</div><div class="label">Production Coverage</div></div>
    `;

    // --- Recommendations ---
    if (DATA.recommendations && DATA.recommendations.length > 0) {{
        const recEl = document.getElementById('recommendations');
        let html = '<div style="background:#1c1208;border:1px solid #d29922;border-radius:6px;padding:16px 24px;margin-bottom:24px">';
        html += '<h3 style="color:#d29922;margin-bottom:8px">Suggested flags to reduce failures</h3>';
        for (const r of DATA.recommendations) {{
            html += `<div style="margin:6px 0"><code style="color:#f0f6fc;background:#21262d;padding:2px 6px;border-radius:3px">${{r.flag}}</code> <span style="color:#8b949e;margin-left:8px">${{r.reason}}</span><div style="color:#6e7681;font-size:0.8rem;margin-left:2px">${{r.estimated_impact}}</div></div>`;
        }}
        html += '</div>';
        recEl.innerHTML = html;
    }}

    // --- Histogram renderer ---
    function renderHistogram(containerId, histogram) {{
        const el = document.getElementById(containerId);
        if (!histogram.buckets || histogram.buckets.length === 0) {{
            el.innerHTML = '<p style="color:#8b949e">No data</p>';
            return;
        }}
        const maxCount = Math.max(...histogram.buckets.map(b => b[1]));
        let html = '';
        for (const [lower, count] of histogram.buckets) {{
            const pct = maxCount > 0 ? (count / maxCount * 100) : 0;
            html += `<div class="bar-row">
                <span class="bar-label">${{lower}}</span>
                <div class="bar" style="width:${{Math.max(pct, 0.5)}}%;background:#238636"></div>
                <span class="bar-value">${{count.toLocaleString()}}</span>
            </div>`;
        }}
        el.innerHTML = html;
    }}

    renderHistogram('depth-chart', S.depth_histogram);
    renderHistogram('node-chart', S.node_count_histogram);

    // --- Production Table ---
    function renderTable() {{
        const prods = [...S.productions];
        let sortCol = 'hit_count';
        let sortAsc = false;

        function draw() {{
            prods.sort((a, b) => {{
                let va = a[sortCol], vb = b[sortCol];
                if (typeof va === 'string') return sortAsc ? va.localeCompare(vb) : vb.localeCompare(va);
                return sortAsc ? va - vb : vb - va;
            }});

            const total = S.total_payloads || 1;
            let html = `<table><thead><tr>
                <th data-col="name">Name</th>
                <th data-col="hit_count">Hits</th>
                <th data-col="payload_hit_count">Payload Coverage</th>
                <th>Alternatives</th>
            </tr></thead><tbody>`;

            for (const p of prods) {{
                const covPct = (p.payload_hit_count / total * 100).toFixed(1);
                const totalAltHits = p.alternatives.reduce((s, a) => s + a.hit_count, 0) || 1;

                let altBars = '';
                const colors = ['#238636','#1f6feb','#8957e5','#d29922','#da3633','#388bfd','#bc8cff','#3fb950'];
                for (let i = 0; i < p.alternatives.length; i++) {{
                    const a = p.alternatives[i];
                    const w = Math.max(a.hit_count / totalAltHits * 100, 0.5);
                    altBars += `<span class="alt-bar" style="width:${{w}}%;background:${{colors[i % colors.length]}}" title="Alt ${{i}}: ${{a.hit_count}} hits (w=${{a.weight}})"></span>`;
                }}

                let altDetail = '';
                for (let i = 0; i < p.alternatives.length; i++) {{
                    const a = p.alternatives[i];
                    altDetail += `<div style="font-size:0.8rem;color:#8b949e">Alt ${{i}}: ${{a.hit_count.toLocaleString()}} hits, weight=${{a.weight}}</div>`;
                }}

                html += `<tr class="expandable" data-pid="${{p.production_id}}">
                    <td>${{esc(p.name)}}${{p.has_semantic_hook ? ' <span style="color:#d29922">[hook]</span>' : ''}}</td>
                    <td>${{p.hit_count.toLocaleString()}}</td>
                    <td>${{covPct}}% (${{p.payload_hit_count.toLocaleString()}})</td>
                    <td style="width:30%">${{altBars}}</td>
                </tr>
                <tr class="alt-breakdown" data-pid-detail="${{p.production_id}}"><td colspan="4">${{altDetail}}</td></tr>`;
            }}
            html += '</tbody></table>';

            const container = document.getElementById('prod-table');
            container.innerHTML = html;

            // Sort handlers
            container.querySelectorAll('th[data-col]').forEach(th => {{
                th.addEventListener('click', () => {{
                    const col = th.dataset.col;
                    if (sortCol === col) sortAsc = !sortAsc;
                    else {{ sortCol = col; sortAsc = col === 'name'; }}
                    draw();
                }});
            }});

            // Expand/collapse
            container.querySelectorAll('tr.expandable').forEach(tr => {{
                tr.addEventListener('click', () => {{
                    const pid = tr.dataset.pid;
                    const detail = container.querySelector(`tr[data-pid-detail="${{pid}}"]`);
                    const open = detail.style.display === 'table-row';
                    detail.style.display = open ? 'none' : 'table-row';
                    tr.classList.toggle('open', !open);
                }});
            }});
        }}
        draw();
    }}
    renderTable();

    // --- Treemap ---
    function renderTreemap() {{
        const container = document.getElementById('treemap');
        const tooltip = document.getElementById('treemap-tooltip');
        const W = container.clientWidth;
        const H = container.clientHeight;
        if (W === 0 || H === 0) return;

        const items = S.productions
            .filter(p => p.hit_count > 0)
            .map(p => ({{ ...p, area: p.hit_count }}))
            .sort((a, b) => b.area - a.area);

        if (items.length === 0) {{
            container.innerHTML = '<p style="padding:16px;color:#8b949e">No production hits to display</p>';
            return;
        }}

        const totalArea = items.reduce((s, i) => s + i.area, 0);
        const total = S.total_payloads || 1;

        // Simple squarified treemap (slice-and-dice)
        function layout(items, x, y, w, h) {{
            if (items.length === 0 || w <= 0 || h <= 0) return [];
            if (items.length === 1) {{
                return [{{ item: items[0], x, y, w, h }}];
            }}

            const sum = items.reduce((s, i) => s + i.area, 0);
            let acc = 0;
            let splitIdx = 0;
            const half = sum / 2;
            for (let i = 0; i < items.length; i++) {{
                acc += items[i].area;
                if (acc >= half) {{ splitIdx = i; break; }}
            }}
            splitIdx = Math.max(splitIdx, 0);
            const left = items.slice(0, splitIdx + 1);
            const right = items.slice(splitIdx + 1);
            const leftSum = left.reduce((s, i) => s + i.area, 0);
            const ratio = sum > 0 ? leftSum / sum : 0.5;

            if (w >= h) {{
                const splitX = x + w * ratio;
                return [
                    ...layout(left, x, y, w * ratio, h),
                    ...layout(right, splitX, y, w * (1 - ratio), h),
                ];
            }} else {{
                const splitY = y + h * ratio;
                return [
                    ...layout(left, x, y, w, h * ratio),
                    ...layout(right, x, splitY, w, h * (1 - ratio)),
                ];
            }}
        }}

        const cells = layout(items, 0, 0, W, H);
        let html = '';
        for (const c of cells) {{
            const covPct = c.item.payload_hit_count / total;
            // Green (high coverage) to red (low coverage)
            const g = Math.round(covPct * 200).toString(16).padStart(2, '0');
            const r = Math.round((1 - covPct) * 200).toString(16).padStart(2, '0');
            const color = `#${{r}}${{g}}20`;

            if (c.w > 2 && c.h > 2) {{
                const label = c.w > 50 && c.h > 16 ? c.item.name : '';
                html += `<div class="treemap-cell" data-name="${{esc(c.item.name)}}" data-hits="${{c.item.hit_count}}" data-cov="${{(covPct * 100).toFixed(1)}}"
                    style="left:${{c.x}}px;top:${{c.y}}px;width:${{c.w}}px;height:${{c.h}}px;background:${{color}}">${{label}}</div>`;
            }}
        }}
        container.innerHTML = html;

        container.addEventListener('mousemove', e => {{
            const cell = e.target.closest('.treemap-cell');
            if (cell) {{
                tooltip.style.display = 'block';
                tooltip.style.left = (e.clientX + 12) + 'px';
                tooltip.style.top = (e.clientY + 12) + 'px';
                tooltip.innerHTML = `<strong>${{cell.dataset.name}}</strong><br>Hits: ${{Number(cell.dataset.hits).toLocaleString()}}<br>Coverage: ${{cell.dataset.cov}}%`;
            }} else {{
                tooltip.style.display = 'none';
            }}
        }});
        container.addEventListener('mouseleave', () => {{ tooltip.style.display = 'none'; }});
    }}
    renderTreemap();

    // --- Hard-to-Reach Panel ---
    function renderIssues() {{
        let items = [];

        for (const p of R.unreached) {{
            items.push(`<li><span class="tag tag-unreached">UNREACHED</span><strong>${{esc(p.name)}}</strong>
                <div class="explanation">This production was never hit in any payload.</div></li>`);
        }}
        for (const p of R.low_coverage) {{
            items.push(`<li><span class="tag tag-low">LOW</span><strong>${{esc(p.name)}}</strong>
                <div class="explanation">Hit in only ${{p.coverage_pct.toFixed(2)}}% of payloads.</div></li>`);
        }}
        for (const a of R.starved_alternatives) {{
            items.push(`<li><span class="tag tag-starved">STARVED ALT</span><strong>${{esc(a.production_name)}}</strong> alt ${{a.alt_index}}
                <div class="explanation">${{a.reason}} (expected ~${{Math.round(a.expected_uniform)}} uniform hits)</div></li>`);
        }}
        for (const c of R.choke_points) {{
            items.push(`<li><span class="tag tag-choke">CHOKE POINT</span><strong>${{esc(c.production_name)}}</strong>
                <div class="explanation">Only reachable via <strong>${{esc(c.parent_name)}}</strong> alt ${{c.parent_alt_index}}. If that path is cold, this production is unreachable.</div></li>`);
        }}
        for (const w of R.weight_disadvantaged) {{
            items.push(`<li><span class="tag tag-weight">LOW WEIGHT</span><strong>${{esc(w.production_name)}}</strong> alt ${{w.alt_index}}
                <div class="explanation">Weight ${{w.weight}} vs max sibling weight ${{w.max_sibling_weight}} (&lt;25% of max).</div></li>`);
        }}

        const el = document.getElementById('issues');
        if (items.length === 0) {{
            el.innerHTML = '<p style="color:#3fb950">All productions and alternatives have healthy coverage.</p>';
        }} else {{
            el.innerHTML = `<ul class="issue-list">${{items.join('')}}</ul>`;
        }}
    }}
    renderIssues();
}})();
</script>
</body>
</html>"##
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reachability::ReachabilityReport;
    use crate::stats::{AlternativeStats, CorpusStats, FailureBreakdown, Histogram, ProductionStats};

    #[test]
    fn test_render_produces_valid_html() {
        let stats = CorpusStats {
            total_payloads: 10,
            failures: 1,
            failure_breakdown: FailureBreakdown { max_depth: 1, max_total_nodes: 0 },
            productions: vec![ProductionStats {
                name: "root".into(),
                production_id: 0,
                hit_count: 9,
                payload_hit_count: 9,
                alternatives: vec![AlternativeStats {
                    index: 0,
                    weight: 1.0,
                    hit_count: 9,
                }],
                has_semantic_hook: false,
            }],
            depth_histogram: Histogram {
                buckets: vec![(1, 9)],
                min: 1,
                max: 1,
            },
            node_count_histogram: Histogram {
                buckets: vec![(1, 9)],
                min: 1,
                max: 1,
            },
        };

        let reachability = ReachabilityReport::default();

        let html = render(&stats, &reachability, &[], "fixtures/grammars/test.ebnf");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("barkus-viz Coverage Report"));
        assert!(html.contains("\"total_payloads\":10"));
    }
}
