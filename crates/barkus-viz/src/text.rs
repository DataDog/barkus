use std::fmt::Write;

use crate::reachability::ReachabilityReport;
use crate::recommend::Recommendation;
use crate::stats::{CorpusStats, Histogram};

// ANSI helpers
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const MAGENTA: &str = "\x1b[35m";
const CYAN: &str = "\x1b[36m";

const BAR_WIDTH: usize = 40;
const ALT_COLORS: [&str; 6] = [GREEN, BLUE, MAGENTA, YELLOW, RED, CYAN];

pub fn render(
    stats: &CorpusStats,
    reachability: &ReachabilityReport,
    recommendations: &[Recommendation],
    grammar_path: &str,
) -> String {
    let mut out = String::with_capacity(4096);

    render_header(&mut out, stats, grammar_path);
    render_recommendations(&mut out, recommendations, grammar_path);
    render_histogram(&mut out, "Depth Distribution", &stats.depth_histogram);
    render_histogram(&mut out, "Node Count Distribution", &stats.node_count_histogram);
    render_production_table(&mut out, stats);
    render_issues(&mut out, reachability);

    out
}

fn render_header(out: &mut String, stats: &CorpusStats, grammar_path: &str) {
    let total = stats.total_payloads;
    let failures = stats.failures;
    let fail_rate = if total > 0 {
        failures as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    let covered = stats.productions.iter().filter(|p| p.payload_hit_count > 0).count();
    let total_prods = stats.productions.len();
    let coverage_pct = if total_prods > 0 {
        covered as f64 / total_prods as f64 * 100.0
    } else {
        0.0
    };

    let _ = writeln!(out, "{BOLD}{CYAN}barkus-viz Coverage Report{RESET}");
    let _ = writeln!(out, "{DIM}Grammar: {grammar_path}{RESET}");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  Payloads:            {BOLD}{}{RESET}",
        fmt_num(total)
    );
    let _ = writeln!(
        out,
        "  Failure rate:        {BOLD}{fail_rate:.2}%{RESET}  ({} failures)",
        fmt_num(failures)
    );
    if failures > 0 {
        let bd = &stats.failure_breakdown;
        let _ = writeln!(
            out,
            "    {DIM}├ max depth exceeded:      {}{RESET}",
            fmt_num(bd.max_depth)
        );
        let _ = writeln!(
            out,
            "    {DIM}└ max total nodes exceeded: {}{RESET}",
            fmt_num(bd.max_total_nodes)
        );
    }
    let _ = writeln!(
        out,
        "  Production coverage: {BOLD}{coverage_pct:.1}%{RESET}  ({covered} / {total_prods} hit)",
    );
    let _ = writeln!(out);
}

fn render_recommendations(out: &mut String, recs: &[Recommendation], grammar_path: &str) {
    if recs.is_empty() {
        return;
    }

    let _ = writeln!(out, "{BOLD}{YELLOW}Suggested flags to reduce failures:{RESET}");
    for rec in recs {
        let _ = writeln!(out, "    {BOLD}{:<24}{RESET} {DIM}{}{RESET}", rec.flag, rec.reason);
        let _ = writeln!(out, "    {:<24} {DIM}{}{RESET}", "", rec.estimated_impact);
    }
    let _ = writeln!(out);

    let flags: Vec<&str> = recs.iter().map(|r| r.flag.as_str()).collect();
    let _ = writeln!(out, "  {DIM}Full command:{RESET}");
    let _ = writeln!(
        out,
        "    cargo run -p barkus-viz -- {grammar_path} {}",
        flags.join(" "),
    );
    let _ = writeln!(out);
}

fn render_histogram(out: &mut String, title: &str, histogram: &Histogram) {
    let _ = writeln!(out, "{BOLD}{title}{RESET}");

    if histogram.buckets.is_empty() {
        let _ = writeln!(out, "  {DIM}No data{RESET}");
        let _ = writeln!(out);
        return;
    }

    let max_count = histogram.buckets.iter().map(|(_, c)| *c).max().unwrap_or(1);
    let max_label_width = histogram
        .buckets
        .iter()
        .map(|(lower, _)| lower.to_string().len())
        .max()
        .unwrap_or(1);

    for &(lower, count) in &histogram.buckets {
        let bar_len = if max_count > 0 {
            (count as f64 / max_count as f64 * BAR_WIDTH as f64).round() as usize
        } else {
            0
        }
        .max(if count > 0 { 1 } else { 0 });

        let bar: String = "█".repeat(bar_len);
        let pad: String = "░".repeat(BAR_WIDTH - bar_len);

        let _ = writeln!(
            out,
            "  {lower:>width$} {GREEN}{bar}{RESET}{DIM}{pad}{RESET} {count}",
            width = max_label_width,
        );
    }
    let _ = writeln!(
        out,
        "  {DIM}range: {} – {}{RESET}",
        histogram.min, histogram.max
    );
    let _ = writeln!(out);
}

fn render_production_table(out: &mut String, stats: &CorpusStats) {
    let _ = writeln!(out, "{BOLD}Production Coverage{RESET}");

    if stats.productions.is_empty() {
        let _ = writeln!(out, "  {DIM}No productions{RESET}");
        let _ = writeln!(out);
        return;
    }

    let total = stats.total_payloads.max(1);

    // Compute column widths
    let max_name_len = stats
        .productions
        .iter()
        .map(|p| p.name.len() + if p.has_semantic_hook { 7 } else { 0 })
        .max()
        .unwrap_or(4)
        .max(4);
    let name_width = max_name_len.min(30);

    // Alt color legend
    let mut legend = String::from("  Alt colors: ");
    for (i, &color) in ALT_COLORS.iter().enumerate() {
        if i > 0 {
            legend.push_str("  ");
        }
        let _ = write!(legend, "{color}▓{RESET} alt {i}");
    }
    let _ = writeln!(out, "{legend}");
    let _ = writeln!(out);

    // Header
    let _ = writeln!(
        out,
        "  {BOLD}{:<nw$}  {:>10}  {:>8}  {}{RESET}",
        "Name",
        "Hits",
        "Cov %",
        "Alt distribution",
        nw = name_width,
    );
    let _ = writeln!(
        out,
        "  {DIM}{}{RESET}",
        "─".repeat(name_width + 10 + 8 + 20 + 6),
    );

    // Sort by hit count descending
    let mut prods: Vec<_> = stats.productions.iter().collect();
    prods.sort_by(|a, b| b.hit_count.cmp(&a.hit_count));

    let alt_bar_width = 20;

    for p in &prods {
        let cov_pct = p.payload_hit_count as f64 / total as f64 * 100.0;

        let mut name_display = p.name.clone();
        if p.has_semantic_hook {
            name_display.push_str(" [hook]");
        }
        if name_display.len() > name_width {
            name_display.truncate(name_width - 1);
            name_display.push('…');
        }

        // Alt distribution bar
        let alt_bar = render_alt_bar(&p.alternatives, alt_bar_width);

        let cov_color = if cov_pct >= 50.0 {
            GREEN
        } else if cov_pct >= 10.0 {
            YELLOW
        } else {
            RED
        };

        let _ = writeln!(
            out,
            "  {:<nw$}  {:>10}  {cov_color}{:>7.1}%{RESET}  {alt_bar}",
            name_display,
            fmt_num(p.hit_count),
            cov_pct,
            nw = name_width,
        );
    }
    let _ = writeln!(out);
}

fn render_alt_bar(alternatives: &[crate::stats::AlternativeStats], width: usize) -> String {
    if alternatives.len() <= 1 {
        return format!("{DIM}(single){RESET}");
    }

    let total_hits: u64 = alternatives.iter().map(|a| a.hit_count).sum();
    if total_hits == 0 {
        return format!("{DIM}(no hits){RESET}");
    }

    let mut bar = String::new();

    for (i, alt) in alternatives.iter().enumerate() {
        let frac = alt.hit_count as f64 / total_hits as f64;
        let len = (frac * width as f64).round() as usize;
        let len = if alt.hit_count > 0 { len.max(1) } else { 0 };
        let color = ALT_COLORS[i % ALT_COLORS.len()];
        let _ = write!(bar, "{color}{}{RESET}", "▓".repeat(len));
    }

    bar
}

fn render_issues(out: &mut String, r: &ReachabilityReport) {
    let _ = writeln!(out, "{BOLD}Hard-to-Reach Analysis{RESET}");

    let has_any = !r.unreached.is_empty()
        || !r.low_coverage.is_empty()
        || !r.starved_alternatives.is_empty()
        || !r.choke_points.is_empty()
        || !r.weight_disadvantaged.is_empty();

    if !has_any {
        let _ = writeln!(out, "  {GREEN}All productions and alternatives have healthy coverage.{RESET}");
        let _ = writeln!(out);
        return;
    }

    let issues: Vec<(&str, &str, String, String)> = r
        .unreached
        .iter()
        .map(|p| (RED, "UNREACHED", p.name.clone(), "Never hit in any payload.".into()))
        .chain(r.low_coverage.iter().map(|p| {
            (YELLOW, "LOW", p.name.clone(), format!("Hit in only {:.2}% of payloads.", p.coverage_pct))
        }))
        .chain(r.starved_alternatives.iter().map(|a| {
            (
                MAGENTA,
                "STARVED",
                format!("{} alt {}", a.production_name, a.alt_index),
                format!("{} (expected ~{:.0} uniform hits)", a.reason, a.expected_uniform),
            )
        }))
        .chain(r.choke_points.iter().map(|c| {
            (
                BLUE,
                "CHOKE",
                c.production_name.clone(),
                format!(
                    "Only reachable via {} alt {}. If that path is cold, this is unreachable.",
                    c.parent_name, c.parent_alt_index
                ),
            )
        }))
        .chain(r.weight_disadvantaged.iter().map(|w| {
            (
                DIM,
                "LOW WEIGHT",
                format!("{} alt {}", w.production_name, w.alt_index),
                format!("Weight {} vs max sibling weight {} (<25% of max).", w.weight, w.max_sibling_weight),
            )
        }))
        .collect();

    for (color, tag, name, explanation) in &issues {
        let _ = writeln!(out, "  {color}{BOLD}{tag:<10}{RESET} {BOLD}{name}{RESET}");
        let _ = writeln!(out, "             {DIM}{explanation}{RESET}");
    }

    let _ = writeln!(out);
}

fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reachability::{
        ChokePoint, LowCoverageProduction, ReachabilityReport, StarvedAlternative,
        UnreachedProduction, WeightDisadvantaged,
    };
    use crate::stats::{AlternativeStats, CorpusStats, FailureBreakdown, Histogram, ProductionStats};

    fn sample_stats() -> CorpusStats {
        CorpusStats {
            total_payloads: 1000,
            failures: 5,
            failure_breakdown: FailureBreakdown { max_depth: 3, max_total_nodes: 2 },
            productions: vec![
                ProductionStats {
                    name: "root".into(),
                    production_id: 0,
                    hit_count: 995,
                    payload_hit_count: 995,
                    alternatives: vec![
                        AlternativeStats {
                            index: 0,
                            weight: 1.0,
                            hit_count: 500,
                        },
                        AlternativeStats {
                            index: 1,
                            weight: 1.0,
                            hit_count: 495,
                        },
                    ],
                    has_semantic_hook: false,
                },
                ProductionStats {
                    name: "leaf".into(),
                    production_id: 1,
                    hit_count: 200,
                    payload_hit_count: 150,
                    alternatives: vec![AlternativeStats {
                        index: 0,
                        weight: 1.0,
                        hit_count: 200,
                    }],
                    has_semantic_hook: false,
                },
            ],
            depth_histogram: Histogram {
                buckets: vec![(1, 100), (3, 500), (5, 300), (7, 95)],
                min: 1,
                max: 7,
            },
            node_count_histogram: Histogram {
                buckets: vec![(1, 50), (10, 800), (20, 145)],
                min: 1,
                max: 20,
            },
        }
    }

    #[test]
    fn test_render_contains_sections() {
        let stats = sample_stats();
        let reach = ReachabilityReport::default();
        let output = render(&stats, &reach, &[], "test.ebnf");

        assert!(output.contains("barkus-viz Coverage Report"));
        assert!(output.contains("test.ebnf"));
        assert!(output.contains("1,000"));
        assert!(output.contains("Depth Distribution"));
        assert!(output.contains("Node Count Distribution"));
        assert!(output.contains("Production Coverage"));
        assert!(output.contains("Hard-to-Reach Analysis"));
        assert!(output.contains("healthy coverage"));
    }

    #[test]
    fn test_render_with_issues() {
        let stats = sample_stats();
        let reach = ReachabilityReport {
            unreached: vec![UnreachedProduction {
                name: "dead_rule".into(),
                production_id: 99,
            }],
            low_coverage: vec![LowCoverageProduction {
                name: "rare_rule".into(),
                production_id: 98,
                coverage_pct: 0.3,
            }],
            starved_alternatives: vec![StarvedAlternative {
                production_name: "choice".into(),
                production_id: 10,
                alt_index: 2,
                hit_count: 5,
                expected_uniform: 100.0,
                reason: "hit 5 times, expected ~100 (< 50% of uniform)".into(),
            }],
            choke_points: vec![ChokePoint {
                production_name: "narrow".into(),
                production_id: 20,
                parent_name: "parent".into(),
                parent_id: 0,
                parent_alt_index: 1,
            }],
            weight_disadvantaged: vec![WeightDisadvantaged {
                production_name: "biased".into(),
                production_id: 30,
                alt_index: 0,
                weight: 0.1,
                max_sibling_weight: 1.0,
            }],
        };

        let output = render(&stats, &reach, &[], "test.ebnf");

        assert!(output.contains("UNREACHED"));
        assert!(output.contains("dead_rule"));
        assert!(output.contains("LOW"));
        assert!(output.contains("rare_rule"));
        assert!(output.contains("STARVED"));
        assert!(output.contains("choice"));
        assert!(output.contains("CHOKE"));
        assert!(output.contains("narrow"));
        assert!(output.contains("LOW WEIGHT"));
        assert!(output.contains("biased"));
    }

    #[test]
    fn test_fmt_num() {
        assert_eq!(fmt_num(0), "0");
        assert_eq!(fmt_num(999), "999");
        assert_eq!(fmt_num(1000), "1,000");
        assert_eq!(fmt_num(1_000_000), "1,000,000");
    }

    #[test]
    fn test_render_empty_stats() {
        let stats = CorpusStats {
            total_payloads: 0,
            failures: 0,
            failure_breakdown: FailureBreakdown { max_depth: 0, max_total_nodes: 0 },
            productions: vec![],
            depth_histogram: Histogram {
                buckets: vec![],
                min: u32::MAX,
                max: 0,
            },
            node_count_histogram: Histogram {
                buckets: vec![],
                min: u32::MAX,
                max: 0,
            },
        };
        let reach = ReachabilityReport::default();
        let output = render(&stats, &reach, &[], "empty.ebnf");
        assert!(output.contains("No data"));
        assert!(output.contains("No productions"));
    }
}
