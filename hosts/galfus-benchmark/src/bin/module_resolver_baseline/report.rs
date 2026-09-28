#[path = "report/charts.rs"]
mod charts;

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use sysinfo::System;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct BaselineSample {
    pub(super) workspace_discovery_us: u64,
    pub(super) check_us: u64,
    pub(super) compile_us: u64,
    pub(super) package_encode_us: u64,
    pub(super) runtime_start_us: u64,
    pub(super) entry_completion_us: u64,
    pub(super) pipeline_us: u64,
    pub(super) process_total_us: u64,
    pub(super) package_bytes: usize,
    pub(super) cataloged_modules: usize,
    pub(super) materialized_modules: usize,
    pub(super) decoded_chunks: usize,
    pub(super) exit_code: i32,
    pub(super) peak_rss_bytes: u64,
}

#[derive(Debug, Serialize)]
pub(super) struct FixtureReport {
    pub(super) fixture: String,
    pub(super) workspace_lazy: ExecutionReport,
    pub(super) standalone_eager: ExecutionReport,
}

#[derive(Debug, Serialize)]
pub(super) struct ExecutionReport {
    pub(super) samples: Vec<BaselineSample>,
    pub(super) metrics: BaselineStatistics,
}

#[derive(Debug, Serialize)]
pub(super) struct BaselineStatistics {
    median: BaselineMetrics,
    minimum: BaselineMetrics,
    maximum: BaselineMetrics,
}

#[derive(Debug, Clone, Serialize)]
struct BaselineMetrics {
    workspace_discovery_us: u64,
    check_us: u64,
    compile_us: u64,
    package_encode_us: u64,
    runtime_start_us: u64,
    entry_completion_us: u64,
    pipeline_us: u64,
    process_total_us: u64,
    package_bytes: usize,
    cataloged_modules: usize,
    materialized_modules: usize,
    decoded_chunks: usize,
    peak_rss_bytes: u64,
}

#[derive(Debug, Deserialize)]
pub(super) struct PhaseOneBaselineReport {
    environment: PhaseOneEnvironment,
    fixtures: Vec<PhaseOneFixtureReport>,
}

#[derive(Debug, Deserialize)]
struct PhaseOneEnvironment {
    git_revision: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PhaseOneFixtureReport {
    fixture: String,
    median: PhaseOneMetrics,
}

#[derive(Debug, Deserialize)]
struct PhaseOneMetrics {
    runtime_start_us: u64,
    process_total_us: u64,
    package_bytes: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct PhaseOneComparison {
    source: String,
    git_revision: Option<String>,
    fixtures: Vec<PhaseOneFixtureComparison>,
}

#[derive(Debug, Serialize)]
struct PhaseOneFixtureComparison {
    fixture: String,
    phase_one_runtime_start_us: u64,
    standalone_eager_runtime_start_us: u64,
    runtime_start_delta_us: i64,
    phase_one_process_total_us: u64,
    standalone_eager_process_total_us: u64,
    process_total_delta_us: i64,
    phase_one_package_bytes: usize,
    standalone_eager_package_bytes: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct BenchmarkEnvironment {
    git_revision: Option<String>,
    working_tree_dirty: bool,
    operating_system: String,
    architecture: String,
    cpu: Option<String>,
    rustc: Option<String>,
    cargo: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct BaselineReport {
    pub(super) schema_version: u8,
    pub(super) generated_unix_seconds: u64,
    pub(super) command: String,
    pub(super) sample_count: usize,
    pub(super) environment: BenchmarkEnvironment,
    pub(super) fixtures: Vec<FixtureReport>,
    pub(super) phase_one_comparison: PhaseOneComparison,
}

#[derive(Clone, Copy)]
enum StatisticPoint {
    Median,
    Minimum,
    Maximum,
}

pub(super) fn statistics(samples: &[BaselineSample]) -> BaselineStatistics {
    BaselineStatistics {
        median: metrics(samples, StatisticPoint::Median),
        minimum: metrics(samples, StatisticPoint::Minimum),
        maximum: metrics(samples, StatisticPoint::Maximum),
    }
}

fn metrics(samples: &[BaselineSample], point: StatisticPoint) -> BaselineMetrics {
    BaselineMetrics {
        workspace_discovery_us: metric_by(samples, |sample| sample.workspace_discovery_us, point),
        check_us: metric_by(samples, |sample| sample.check_us, point),
        compile_us: metric_by(samples, |sample| sample.compile_us, point),
        package_encode_us: metric_by(samples, |sample| sample.package_encode_us, point),
        runtime_start_us: metric_by(samples, |sample| sample.runtime_start_us, point),
        entry_completion_us: metric_by(samples, |sample| sample.entry_completion_us, point),
        pipeline_us: metric_by(samples, |sample| sample.pipeline_us, point),
        process_total_us: metric_by(samples, |sample| sample.process_total_us, point),
        package_bytes: metric_by(samples, |sample| sample.package_bytes, point),
        cataloged_modules: metric_by(samples, |sample| sample.cataloged_modules, point),
        materialized_modules: metric_by(samples, |sample| sample.materialized_modules, point),
        decoded_chunks: metric_by(samples, |sample| sample.decoded_chunks, point),
        peak_rss_bytes: metric_by(samples, |sample| sample.peak_rss_bytes, point),
    }
}

fn metric_by<T: Ord + Copy>(
    samples: &[BaselineSample],
    value: impl Fn(&BaselineSample) -> T,
    point: StatisticPoint,
) -> T {
    let mut values = samples.iter().map(value).collect::<Vec<_>>();
    values.sort_unstable();
    match point {
        StatisticPoint::Median => values[values.len() / 2],
        StatisticPoint::Minimum => values[0],
        StatisticPoint::Maximum => values[values.len() - 1],
    }
}

pub(super) fn load_phase_one_baseline(path: &Path) -> Result<PhaseOneBaselineReport, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "could not read phase-one baseline {}: {error}",
            path.display()
        )
    })?;
    serde_json::from_slice(bytes.as_slice()).map_err(|error| {
        format!(
            "could not decode phase-one baseline {}: {error}",
            path.display()
        )
    })
}

pub(super) fn compare_with_phase_one(
    path: &Path,
    phase_one: &PhaseOneBaselineReport,
    fixtures: &[FixtureReport],
) -> Result<PhaseOneComparison, String> {
    let fixtures = fixtures
        .iter()
        .map(|current| {
            let phase_one_fixture = phase_one
                .fixtures
                .iter()
                .find(|fixture| fixture.fixture == current.fixture)
                .ok_or_else(|| {
                    format!(
                        "phase-one baseline {} has no fixture {}",
                        path.display(),
                        current.fixture
                    )
                })?;
            let current_metrics = &current.standalone_eager.metrics.median;
            Ok(PhaseOneFixtureComparison {
                fixture: current.fixture.clone(),
                phase_one_runtime_start_us: phase_one_fixture.median.runtime_start_us,
                standalone_eager_runtime_start_us: current_metrics.runtime_start_us,
                runtime_start_delta_us: signed_delta(
                    current_metrics.runtime_start_us,
                    phase_one_fixture.median.runtime_start_us,
                ),
                phase_one_process_total_us: phase_one_fixture.median.process_total_us,
                standalone_eager_process_total_us: current_metrics.process_total_us,
                process_total_delta_us: signed_delta(
                    current_metrics.process_total_us,
                    phase_one_fixture.median.process_total_us,
                ),
                phase_one_package_bytes: phase_one_fixture.median.package_bytes,
                standalone_eager_package_bytes: current_metrics.package_bytes,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PhaseOneComparison {
        source: path.display().to_string(),
        git_revision: phase_one.environment.git_revision.clone(),
        fixtures,
    })
}

fn signed_delta(current: u64, baseline: u64) -> i64 {
    current as i64 - baseline as i64
}

pub(super) fn write_report(
    report: &BaselineReport,
    repository_root: &Path,
) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let output_dir = repository_root.join(".tmp").join("benchmark");
    fs::create_dir_all(output_dir.as_path()).map_err(|error| error.to_string())?;
    let prefix = format!("module-resolver-baseline-{}", report.generated_unix_seconds);
    let json_path = output_dir.join(format!("{prefix}.json"));
    let markdown_path = output_dir.join(format!("{prefix}.md"));
    let html_path = output_dir.join(format!("{prefix}.html"));
    let json = serde_json::to_vec_pretty(report).map_err(|error| error.to_string())?;
    fs::write(json_path.as_path(), json).map_err(|error| error.to_string())?;
    fs::write(markdown_path.as_path(), markdown_report(report))
        .map_err(|error| error.to_string())?;
    fs::write(
        html_path.as_path(),
        html_report(report, json_path.as_path(), markdown_path.as_path()),
    )
    .map_err(|error| error.to_string())?;
    Ok((json_path, markdown_path, html_path))
}

fn markdown_report(report: &BaselineReport) -> String {
    let mut output = String::from("# Module resolver cold-start report\n\n");
    output.push_str(&format!("Command: {}\n\n", report.command));
    output.push_str(&format!("Samples per fixture: {}\n\n", report.sample_count));
    output.push_str("Each cell is median [minimum, maximum]. Durations are milliseconds; RSS is bytes. Workspace lazy does not create or decode a package, so its package byte and decoded chunk values are zero. A zero RSS sample means the OS process ended before the monitor observed it.\n\n");
    output.push_str("| Fixture | Mode | Discovery | Check | Compile | Encode | Runtime start | Entry completion | Pipeline | Process total | Package bytes | Cataloged | Materialized | Decoded chunks | Peak RSS bytes |\n");
    output.push_str(
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    );
    for fixture in &report.fixtures {
        append_execution_row(
            &mut output,
            fixture.fixture.as_str(),
            "workspace-lazy",
            &fixture.workspace_lazy.metrics,
        );
        append_execution_row(
            &mut output,
            fixture.fixture.as_str(),
            "standalone-eager",
            &fixture.standalone_eager.metrics,
        );
    }
    output.push_str("\n## Phase 1 comparison and release gate\n\n");
    output.push_str(&format!(
        "Phase 1 source: {} (revision {}).\n\n",
        report.phase_one_comparison.source,
        report
            .phase_one_comparison
            .git_revision
            .as_deref()
            .unwrap_or("unavailable")
    ));
    output.push_str("| Fixture | Phase 1 eager start | Current eager start | Start delta | Phase 1 process | Current process | Process delta | Phase 1 bytes | Current bytes |\n");
    output.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for fixture in &report.phase_one_comparison.fixtures {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            fixture.fixture,
            milliseconds(fixture.phase_one_runtime_start_us),
            milliseconds(fixture.standalone_eager_runtime_start_us),
            signed_milliseconds(fixture.runtime_start_delta_us),
            milliseconds(fixture.phase_one_process_total_us),
            milliseconds(fixture.standalone_eager_process_total_us),
            signed_milliseconds(fixture.process_total_delta_us),
            fixture.phase_one_package_bytes,
            fixture.standalone_eager_package_bytes,
        ));
    }
    output.push_str("\nRelease gate: any standalone-eager cold-start or process-time regression against this Phase 1 baseline requires an explicit release-note explanation. This gate intentionally uses the measured baseline and sets no synthetic percentage target.\n");
    output.push_str("No production optimization is selected by this report: any future optimization must attribute its benefit to direct ID lookup, canonical decoding, or a compiler pass with a measured before/after result.\n");
    output.push_str("\n## Environment\n\n");
    output.push_str(&format!("- Git revision: {}\n", git_reference(report)));
    output.push_str(&format!(
        "- Platform: {} {}\n",
        report.environment.operating_system, report.environment.architecture
    ));
    output.push_str(&format!(
        "- CPU: {}\n",
        report.environment.cpu.as_deref().unwrap_or("unavailable")
    ));
    output.push_str(&format!(
        "- rustc: {}\n",
        report.environment.rustc.as_deref().unwrap_or("unavailable")
    ));
    output.push_str(&format!(
        "- cargo: {}\n",
        report.environment.cargo.as_deref().unwrap_or("unavailable")
    ));
    output
}

fn html_report(report: &BaselineReport, json_path: &Path, markdown_path: &Path) -> String {
    let json_name = json_path.file_name().map_or_else(
        || "report.json".to_string(),
        |name| name.to_string_lossy().into(),
    );
    let markdown_name = markdown_path.file_name().map_or_else(
        || "report.md".to_string(),
        |name| name.to_string_lossy().into(),
    );
    let title = format!(
        "Galfus cold-start benchmark · {} samples",
        report.sample_count
    );
    let plots = [
        ("runtime-start", charts::runtime_start_plot(report)),
        ("stage-composition", charts::stage_composition_plot(report)),
        (
            "process-distribution",
            charts::process_distribution_plot(report),
        ),
        ("materialization", charts::materialization_plot(report)),
        (
            "phase-one-comparison",
            charts::phase_one_comparison_plot(report),
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
  <title>{title}</title>
  {plotly_js}
  <style>
    :root {{ color-scheme: light; font-family: Inter, ui-sans-serif, system-ui, sans-serif; color: #152238; background: #f5f7fb; }}
    body {{ margin: 0; }}
    main {{ max-width: 1480px; margin: 0 auto; padding: 32px 24px 48px; }}
    h1, h2 {{ margin: 0; color: #0b1f3a; }}
    h1 {{ font-size: 2rem; }}
    h2 {{ font-size: 1.2rem; margin-bottom: 8px; }}
    p {{ line-height: 1.5; }}
    .lede {{ color: #50627c; max-width: 960px; }}
    .links {{ display: flex; gap: 12px; flex-wrap: wrap; margin: 18px 0 26px; }}
    a {{ color: #125cc5; font-weight: 600; }}
    .grid {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 18px; }}
    .card {{ min-width: 0; background: white; border: 1px solid #dce3ef; border-radius: 12px; padding: 18px; box-shadow: 0 2px 10px #102a4310; overflow: hidden; }}
    .wide {{ grid-column: 1 / -1; }}
    .chart {{ height: 390px; min-width: 0; width: 100%; }}
    .wide .chart {{ height: 470px; }}
    .meta {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(210px, 1fr)); gap: 12px; margin: 20px 0; }}
    .meta div {{ background: #edf3fc; border-radius: 8px; padding: 12px; }}
    .meta strong {{ display: block; font-size: .78rem; color: #52657e; text-transform: uppercase; letter-spacing: .04em; }}
    .meta span {{ display: block; margin-top: 4px; overflow-wrap: anywhere; }}
    .note {{ color: #52657e; font-size: .9rem; }}
    @media (max-width: 760px) {{ main {{ padding: 20px 12px 32px; }} .grid {{ grid-template-columns: minmax(0, 1fr); }} .wide {{ grid-column: auto; }} .chart, .wide .chart {{ height: 360px; }} }}
    @media print {{ main {{ max-width: none; padding: 12px; }} .card {{ break-inside: avoid; box-shadow: none; }} }}
  </style>
</head>
<body>
<main>
  <h1>{title}</h1>
  <p class="lede">Interactive cold-start analysis for the lazy workspace and eager standalone boundaries. Hover a chart for exact values, click a legend item to isolate a series, and use the Plotly toolbar to export an image.</p>
  <div class="links"><a href="{json_name}">Canonical JSON</a><a href="{markdown_name}">Markdown summary</a></div>
  {metadata}
  <section class="grid">
    <article class="card">{runtime_start}</article>
    <article class="card">{materialization}</article>
    <article class="card wide">{stage_composition}</article>
    <article class="card">{process_distribution}</article>
    <article class="card">{phase_one_comparison}</article>
  </section>
  <p class="note">Durations are milliseconds. The report embeds Plotly.js, so it can be opened offline. Print this page from a browser to create a PDF.</p>
</main>
</body>
</html>"#,
        title = escape_html(title.as_str()),
        plotly_js = charts::offline_js_sources(),
        json_name = escape_html(json_name.as_str()),
        markdown_name = escape_html(markdown_name.as_str()),
        metadata = html_metadata(report),
        runtime_start = plots[0],
        stage_composition = plots[1],
        process_distribution = plots[2],
        materialization = plots[3],
        phase_one_comparison = plots[4],
    )
}

fn html_metadata(report: &BaselineReport) -> String {
    format!(
        r#"<section class="meta">
  <div><strong>Samples</strong><span>{samples}</span></div>
  <div><strong>Git revision</strong><span>{git_revision}</span></div>
  <div><strong>Platform</strong><span>{platform}</span></div>
  <div><strong>CPU</strong><span>{cpu}</span></div>
  <div><strong>Phase 1 baseline</strong><span>{baseline}</span></div>
</section>"#,
        samples = report.sample_count,
        git_revision = escape_html(git_reference(report).as_str()),
        platform = escape_html(
            format!(
                "{} {}",
                report.environment.operating_system, report.environment.architecture
            )
            .as_str()
        ),
        cpu = escape_html(report.environment.cpu.as_deref().unwrap_or("unavailable")),
        baseline = escape_html(report.phase_one_comparison.source.as_str()),
    )
}

fn git_reference(report: &BaselineReport) -> String {
    let revision = report
        .environment
        .git_revision
        .as_deref()
        .unwrap_or("unavailable");
    if report.environment.working_tree_dirty {
        format!("{revision} (dirty worktree)")
    } else {
        revision.to_string()
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn append_execution_row(
    output: &mut String,
    fixture: &str,
    mode: &str,
    metrics: &BaselineStatistics,
) {
    output.push_str(&format!(
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
        fixture,
        mode,
        duration_range(metrics, |metric| metric.workspace_discovery_us),
        duration_range(metrics, |metric| metric.check_us),
        duration_range(metrics, |metric| metric.compile_us),
        duration_range(metrics, |metric| metric.package_encode_us),
        duration_range(metrics, |metric| metric.runtime_start_us),
        duration_range(metrics, |metric| metric.entry_completion_us),
        duration_range(metrics, |metric| metric.pipeline_us),
        duration_range(metrics, |metric| metric.process_total_us),
        integer_range(metrics, |metric| metric.package_bytes),
        integer_range(metrics, |metric| metric.cataloged_modules),
        integer_range(metrics, |metric| metric.materialized_modules),
        integer_range(metrics, |metric| metric.decoded_chunks),
        integer_range(metrics, |metric| metric.peak_rss_bytes),
    ));
}

fn duration_range(metrics: &BaselineStatistics, value: impl Fn(&BaselineMetrics) -> u64) -> String {
    format!(
        "{} [{}, {}]",
        milliseconds(value(&metrics.median)),
        milliseconds(value(&metrics.minimum)),
        milliseconds(value(&metrics.maximum)),
    )
}

fn integer_range<T: std::fmt::Display>(
    metrics: &BaselineStatistics,
    value: impl Fn(&BaselineMetrics) -> T,
) -> String {
    format!(
        "{} [{}, {}]",
        value(&metrics.median),
        value(&metrics.minimum),
        value(&metrics.maximum),
    )
}

fn milliseconds(microseconds: u64) -> String {
    format!("{:.3}", microseconds as f64 / 1_000.0)
}

fn signed_milliseconds(microseconds: i64) -> String {
    format!("{:+.3}", microseconds as f64 / 1_000.0)
}

pub(super) fn benchmark_environment() -> BenchmarkEnvironment {
    let system = System::new_all();
    BenchmarkEnvironment {
        git_revision: command_output("git", &["rev-parse", "HEAD"]),
        working_tree_dirty: command_output(
            "git",
            &["status", "--porcelain", "--untracked-files=no"],
        )
        .is_some_and(|status| !status.is_empty()),
        operating_system: env::consts::OS.to_string(),
        architecture: env::consts::ARCH.to_string(),
        cpu: system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .filter(|brand| !brand.is_empty()),
        rustc: command_output("rustc", &["--version"]),
        cargo: command_output("cargo", &["--version"]),
    }
}

fn command_output(command: &str, arguments: &[&str]) -> Option<String> {
    Command::new(command)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|output| output.trim().to_string())
}

pub(super) fn unix_timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())
        .map(|duration| duration.as_secs())
}
