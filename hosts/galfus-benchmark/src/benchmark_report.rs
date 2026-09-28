use super::{BenchmarkEnvironment, BenchmarkResult};
use plotly::layout::{Axis, BarMode, Margin};
use plotly::{Bar, Configuration, Layout, Plot};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn write_html_report(
    output_dir: &Path,
    timestamp: u64,
    environment: &BenchmarkEnvironment,
    summaries: &[BenchmarkResult],
) -> Result<PathBuf, String> {
    if summaries.is_empty() {
        return Err("no completed benchmark summaries are available".to_string());
    }
    let path = output_dir.join(format!("{timestamp}-language-comparison.html"));
    fs::write(path.as_path(), html_report(environment, summaries))
        .map_err(|error| error.to_string())?;
    Ok(path)
}

fn html_report(environment: &BenchmarkEnvironment, summaries: &[BenchmarkResult]) -> String {
    let plots = [
        (
            "total-time",
            comparison_plot(summaries, "Median process time", "Milliseconds", |result| {
                result.total_time_ms as f64
            }),
        ),
        (
            "cold-start",
            comparison_plot(
                summaries,
                "Estimated cold start",
                "Milliseconds",
                |result| result.cold_start_ms as f64,
            ),
        ),
        (
            "memory",
            comparison_plot(summaries, "Peak resident memory", "MiB", |result| {
                result.peak_rss_mb
            }),
        ),
    ]
    .into_iter()
    .map(|(id, plot)| {
        format!(
            r#"<div class="chart">{}</div>"#,
            plot.to_inline_html(Some(id))
        )
    })
    .collect::<Vec<_>>();

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Galfus language benchmark comparison</title>
  {plotly_js}
  <style>
    :root {{ color-scheme: light; font-family: Inter, ui-sans-serif, system-ui, sans-serif; color: #152238; background: #f5f7fb; }}
    body {{ margin: 0; }}
    main {{ max-width: 1480px; margin: 0 auto; padding: 32px 24px 48px; }}
    h1 {{ margin: 0; color: #0b1f3a; font-size: 2rem; }}
    p {{ line-height: 1.5; }}
    .lede, .note {{ color: #50627c; max-width: 1000px; }}
    .meta {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; margin: 22px 0; }}
    .meta div {{ background: #edf3fc; border-radius: 8px; padding: 12px; }}
    .meta strong {{ display: block; font-size: .78rem; color: #52657e; text-transform: uppercase; letter-spacing: .04em; }}
    .meta span {{ display: block; margin-top: 4px; overflow-wrap: anywhere; }}
    .card {{ min-width: 0; background: white; border: 1px solid #dce3ef; border-radius: 12px; padding: 18px; margin-top: 18px; box-shadow: 0 2px 10px #102a4310; overflow: hidden; }}
    .chart {{ height: 500px; min-width: 0; width: 100%; }}
    .table-wrap {{ overflow-x: auto; }}
    table {{ border-collapse: collapse; min-width: 960px; width: 100%; }}
    th, td {{ border-bottom: 1px solid #dce3ef; padding: 9px; text-align: right; white-space: nowrap; }}
    th:first-child, td:first-child {{ text-align: left; }}
    details {{ margin-top: 18px; color: #50627c; }}
    li {{ overflow-wrap: anywhere; }}
    @media (max-width: 760px) {{ main {{ padding: 20px 12px 32px; }} .chart {{ height: 390px; }} }}
    @media print {{ main {{ max-width: none; padding: 12px; }} .card {{ break-inside: avoid; box-shadow: none; }} }}
  </style>
</head>
<body>
<main>
  <h1>Galfus language benchmark comparison</h1>
  <p class="lede">Median values from identical benchmark workloads. Select legend items to isolate a runtime and hover any bar for its exact result. Missing bars mean the target was unavailable or did not complete successfully.</p>
  {metadata}
  <section class="card">{total_time}</section>
  <section class="card">{cold_start}</section>
  <section class="card">{memory}</section>
  {workspace_timing}
  <p class="note">Cold start is estimated as process median minus reported script time. It is useful for comparison inside the same workload, but is not equivalent to the module-resolver cold-start benchmark.</p>
  <details><summary>Detected benchmark commands</summary><ul>{commands}</ul></details>
</main>
</body>
</html>"#,
        plotly_js = Plot::offline_js_sources(),
        metadata = metadata(environment, summaries),
        total_time = plots[0],
        cold_start = plots[1],
        memory = plots[2],
        workspace_timing = workspace_timing(summaries),
        commands = command_versions(environment),
    )
}

fn workspace_timing(summaries: &[BenchmarkResult]) -> String {
    let rows = summaries
        .iter()
        .filter(|result| result.language == "Galfus (Workspace)")
        .filter_map(|result| {
            result.workspace_timing.as_ref().map(|timing| {
                format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    escape_html(result.benchmark.as_str()),
                    milliseconds(timing.process_bootstrap_us),
                    milliseconds(timing.manifest_catalog_setup_us),
                    milliseconds(timing.source_discovery_us),
                    milliseconds(timing.source_reads_us),
                    milliseconds(timing.interface_validation_us),
                    optional_milliseconds(timing.first_source_node_production_us),
                    optional_milliseconds(timing.runtime_start_overhead_us),
                )
            })
        })
        .collect::<String>();
    if rows.is_empty() {
        return String::new();
    }
    format!(
        r#"<section class="card"><h2>Galfus workspace phase medians</h2>
<p class="note">Measured by the CLI timing sink. Runtime-start overhead excludes the first source-node production; entry completion remains in the raw JSON.</p>
<div class="table-wrap"><table><thead><tr><th>Workload</th><th>Bootstrap</th><th>Manifest/catalog</th><th>Discovery</th><th>Reads</th><th>Interface validation</th><th>First node</th><th>Runtime start</th></tr></thead><tbody>{rows}</tbody></table></div></section>"#,
    )
}

fn milliseconds(micros: u64) -> String {
    format!("{:.3} ms", micros as f64 / 1_000.0)
}

fn optional_milliseconds(micros: Option<u64>) -> String {
    micros.map(milliseconds).unwrap_or_else(|| "—".to_string())
}

fn comparison_plot(
    summaries: &[BenchmarkResult],
    title: &str,
    unit: &str,
    metric: impl Fn(&BenchmarkResult) -> f64,
) -> Plot {
    let benchmarks = values(summaries.iter().map(|result| result.benchmark.as_str()));
    let languages = values(summaries.iter().map(|result| result.language.as_str()));
    let mut plot = Plot::new();
    for language in languages {
        let values = benchmarks
            .iter()
            .map(|benchmark| {
                summaries
                    .iter()
                    .find(|result| result.benchmark == *benchmark && result.language == language)
                    .map(&metric)
            })
            .collect::<Vec<Option<f64>>>();
        plot.add_trace(Bar::new(benchmarks.clone(), values).name(language));
    }
    plot.set_configuration(Configuration::new().display_logo(false).responsive(true));
    plot.set_layout(
        Layout::new()
            .title(title)
            .auto_size(true)
            .bar_mode(BarMode::Group)
            .margin(Margin::new().left(72).right(24).top(64).bottom(116))
            .x_axis(Axis::new().auto_margin(true))
            .y_axis(Axis::new().title(unit).auto_margin(true)),
    );
    plot
}

fn values<'a>(items: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut values = items
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    values.sort_by_key(|value| (!value.starts_with("Galfus"), value.clone()));
    values
}

fn metadata(environment: &BenchmarkEnvironment, summaries: &[BenchmarkResult]) -> String {
    let workloads = values(summaries.iter().map(|result| result.benchmark.as_str())).len();
    let languages = values(summaries.iter().map(|result| result.language.as_str())).len();
    format!(
        r#"<section class="meta">
  <div><strong>Samples per target</strong><span>{samples}</span></div>
  <div><strong>Completed workloads</strong><span>{workloads}</span></div>
  <div><strong>Detected runtimes</strong><span>{languages}</span></div>
  <div><strong>Release binaries</strong><span>{reuse}</span></div>
</section>"#,
        samples = environment.sample_count,
        workloads = workloads,
        languages = languages,
        reuse = if environment.reuse_release_binaries {
            "reused"
        } else {
            "built for this run"
        },
    )
}

fn command_versions(environment: &BenchmarkEnvironment) -> String {
    environment
        .commands
        .iter()
        .map(|command| {
            let version = command.version.as_deref().unwrap_or("unavailable");
            format!(
                "<li><strong>{}</strong>: {}</li>",
                escape_html(command.command.as_str()),
                escape_html(version)
            )
        })
        .collect()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
