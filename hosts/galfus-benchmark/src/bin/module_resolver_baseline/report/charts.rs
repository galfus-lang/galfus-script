use super::{BaselineMetrics, BaselineReport, ExecutionReport};
use plotly::layout::{Axis, BarMode, Margin};
use plotly::{Bar, BoxPlot, Configuration, Layout, Plot, box_plot::BoxPoints};

pub(super) fn offline_js_sources() -> String {
    Plot::offline_js_sources()
}

pub(super) fn runtime_start_plot(report: &BaselineReport) -> Plot {
    let labels = execution_labels(report);
    let values = execution_reports(report)
        .into_iter()
        .map(|(_, _, execution)| micros_to_millis(execution.metrics.median.runtime_start_us))
        .collect();
    let mut plot = Plot::new();
    plot.add_trace(Bar::new(labels, values).name("Runtime start"));
    finish_plot(
        &mut plot,
        chart_layout("Runtime start by boundary", "Milliseconds"),
    );
    plot
}

pub(super) fn stage_composition_plot(report: &BaselineReport) -> Plot {
    let labels = execution_labels(report);
    let stages: [(&str, fn(&BaselineMetrics) -> u64); 6] = [
        ("Discovery", |metrics| metrics.workspace_discovery_us),
        ("Check", |metrics| metrics.check_us),
        ("Compile", |metrics| metrics.compile_us),
        ("Encode", |metrics| metrics.package_encode_us),
        ("Runtime start", |metrics| metrics.runtime_start_us),
        ("Entry completion", |metrics| metrics.entry_completion_us),
    ];
    let executions = execution_reports(report);
    let mut plot = Plot::new();
    for (name, metric) in stages {
        let values = executions
            .iter()
            .map(|(_, _, execution)| micros_to_millis(metric(&execution.metrics.median)))
            .collect();
        plot.add_trace(Bar::new(labels.clone(), values).name(name));
    }
    finish_plot(
        &mut plot,
        chart_layout("Pipeline composition by median", "Milliseconds").bar_mode(BarMode::Stack),
    );
    plot
}

pub(super) fn process_distribution_plot(report: &BaselineReport) -> Plot {
    let mut plot = Plot::new();
    for (fixture, mode, execution) in execution_reports(report) {
        let values = execution
            .samples
            .iter()
            .map(|sample| micros_to_millis(sample.process_total_us))
            .collect();
        plot.add_trace(
            BoxPlot::new(values)
                .name(format!("{fixture} · {mode}"))
                .box_points(BoxPoints::All),
        );
    }
    finish_plot(
        &mut plot,
        chart_layout("Cold-process distribution", "Milliseconds"),
    );
    plot
}

pub(super) fn materialization_plot(report: &BaselineReport) -> Plot {
    let labels = execution_labels(report);
    let executions = execution_reports(report);
    let cataloged = executions
        .iter()
        .map(|(_, _, execution)| execution.metrics.median.cataloged_modules as f64)
        .collect();
    let materialized = executions
        .iter()
        .map(|(_, _, execution)| execution.metrics.median.materialized_modules as f64)
        .collect();
    let decoded_chunks = executions
        .iter()
        .map(|(_, _, execution)| execution.metrics.median.decoded_chunks as f64)
        .collect();
    let mut plot = Plot::new();
    plot.add_trace(Bar::new(labels.clone(), cataloged).name("Cataloged"));
    plot.add_trace(Bar::new(labels.clone(), materialized).name("Materialized"));
    plot.add_trace(Bar::new(labels, decoded_chunks).name("Decoded chunks"));
    finish_plot(
        &mut plot,
        chart_layout("Module materialization", "Modules").bar_mode(BarMode::Group),
    );
    plot
}

pub(super) fn phase_one_comparison_plot(report: &BaselineReport) -> Plot {
    let labels: Vec<String> = report
        .phase_one_comparison
        .fixtures
        .iter()
        .map(|fixture| fixture.fixture.clone())
        .collect();
    let phase_one = report
        .phase_one_comparison
        .fixtures
        .iter()
        .map(|fixture| micros_to_millis(fixture.phase_one_runtime_start_us))
        .collect();
    let current = report
        .phase_one_comparison
        .fixtures
        .iter()
        .map(|fixture| micros_to_millis(fixture.standalone_eager_runtime_start_us))
        .collect();
    let mut plot = Plot::new();
    plot.add_trace(Bar::new(labels.clone(), phase_one).name("Phase 1 eager"));
    plot.add_trace(Bar::new(labels, current).name("Current eager"));
    finish_plot(
        &mut plot,
        chart_layout("Standalone eager start versus Phase 1", "Milliseconds")
            .bar_mode(BarMode::Group),
    );
    plot
}

fn execution_reports(report: &BaselineReport) -> Vec<(&str, &str, &ExecutionReport)> {
    report
        .fixtures
        .iter()
        .flat_map(|fixture| {
            [
                (
                    fixture.fixture.as_str(),
                    "workspace lazy",
                    &fixture.workspace_lazy,
                ),
                (
                    fixture.fixture.as_str(),
                    "standalone eager",
                    &fixture.standalone_eager,
                ),
            ]
        })
        .collect()
}

fn execution_labels(report: &BaselineReport) -> Vec<String> {
    execution_reports(report)
        .into_iter()
        .map(|(fixture, mode, _)| {
            let fixture = fixture.strip_prefix("resolver-").unwrap_or(fixture);
            let mode = match mode {
                "workspace lazy" => "workspace<br>lazy",
                "standalone eager" => "standalone<br>eager",
                _ => mode,
            };
            format!("{fixture}<br>{mode}")
        })
        .collect()
}

fn chart_layout(title: &str, unit: &str) -> Layout {
    Layout::new()
        .title(title)
        .auto_size(true)
        .margin(Margin::new().left(64).right(24).top(64).bottom(72))
        .x_axis(Axis::new().auto_margin(true))
        .y_axis(Axis::new().title(unit).auto_margin(true))
}

fn finish_plot(plot: &mut Plot, layout: Layout) {
    plot.set_configuration(Configuration::new().display_logo(false).responsive(true));
    plot.set_layout(layout);
}

fn micros_to_millis(value: u64) -> f64 {
    value as f64 / 1_000.0
}
