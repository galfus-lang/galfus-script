use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize)]
pub(super) struct ScalingSample {
    pub(super) source_load_us: u64,
    pub(super) interface_check_us: u64,
    pub(super) runtime_start_us: u64,
    pub(super) entry_completion_us: u64,
    pub(super) pipeline_us: u64,
    pub(super) source_loaded_modules: usize,
    pub(super) interface_checked_modules: usize,
    pub(super) bytecode_produced_modules: usize,
    pub(super) exit_code: i32,
}

#[derive(Debug, Serialize)]
pub(super) struct ScalingFixtureReport {
    pub(super) fixture: String,
    pub(super) source_modules: usize,
    pub(super) entry_closure_modules: usize,
    pub(super) scope: String,
    pub(super) samples: Vec<ScalingSample>,
}

#[derive(Debug, Serialize)]
pub(super) struct ScalingReport {
    pub(super) schema_version: u8,
    pub(super) generated_unix_seconds: u64,
    pub(super) command: String,
    pub(super) sample_count: usize,
    pub(super) fixtures: Vec<ScalingFixtureReport>,
}

pub(super) fn write_report(
    report: &ScalingReport,
    repository_root: &Path,
) -> Result<(PathBuf, PathBuf), String> {
    let output_directory = repository_root.join(".tmp").join("benchmark");
    fs::create_dir_all(output_directory.as_path()).map_err(|error| error.to_string())?;
    let prefix = format!(
        "workspace-scaling-baseline-{}",
        report.generated_unix_seconds
    );
    let json_path = output_directory.join(format!("{prefix}.json"));
    let markdown_path = output_directory.join(format!("{prefix}.md"));
    fs::write(
        json_path.as_path(),
        serde_json::to_vec_pretty(report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::write(markdown_path.as_path(), markdown_report(report))
        .map_err(|error| error.to_string())?;
    Ok((json_path, markdown_path))
}

fn markdown_report(report: &ScalingReport) -> String {
    let mut output = String::from("# Workspace scaling baseline\n\n");
    output.push_str(&format!("Command: `{}`\n\n", report.command));
    output.push_str("Each row is one sample. Module counts are user source modules: loaded is resident source input, interface checked is present in the checked module catalog, and bytecode produced is materialized by the runtime resolver. Durations are milliseconds.\n\n");
    output.push_str("| Fixture | Scope | Sources | Entry closure | Source load | Interface check | Runtime start | Entry completion | Pipeline | Loaded | Checked | Produced | Exit |\n");
    output.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for fixture in &report.fixtures {
        for sample in &fixture.samples {
            output.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                fixture.fixture,
                fixture.scope,
                fixture.source_modules,
                fixture.entry_closure_modules,
                milliseconds(sample.source_load_us),
                milliseconds(sample.interface_check_us),
                milliseconds(sample.runtime_start_us),
                milliseconds(sample.entry_completion_us),
                milliseconds(sample.pipeline_us),
                sample.source_loaded_modules,
                sample.interface_checked_modules,
                sample.bytecode_produced_modules,
                sample.exit_code,
            ));
        }
    }
    output
}

fn milliseconds(microseconds: u64) -> String {
    format!("{:.3}", microseconds as f64 / 1_000.0)
}

pub(super) fn unix_timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())
        .map(|duration| duration.as_secs())
}
