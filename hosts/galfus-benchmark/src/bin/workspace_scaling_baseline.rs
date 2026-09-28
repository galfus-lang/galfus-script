#[path = "workspace_scaling_baseline/fixtures.rs"]
mod fixtures;
#[path = "workspace_scaling_baseline/report.rs"]
mod report;

use fixtures::{ClosureScope, build_fixture};
use galfus_host_native::driver::NativeDriver;
use galfus_host_native::native_catalog;
use galfus_host_native::providers::default_providers;
use galfus_runtime::driver::ExecutionDriver;
use galfus_workspace::{LoadResult, ModuleOrigin, Workspace, WorkspaceManifest};
use report::{ScalingFixtureReport, ScalingReport, ScalingSample, unix_timestamp, write_report};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

const MODULE_COUNTS: [usize; 4] = [10, 100, 1_000, 10_000];
const DEFAULT_SAMPLE_COUNT: usize = 1;

fn main() -> std::process::ExitCode {
    match parse_options().and_then(run) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("workspace scaling baseline failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

struct Options {
    sample_count: usize,
    module_count: Option<usize>,
    scope: Option<ClosureScope>,
}

fn parse_options() -> Result<Options, String> {
    let mut sample_count = DEFAULT_SAMPLE_COUNT;
    let mut module_count = None;
    let mut scope = None;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--samples" => {
                sample_count = arguments
                    .next()
                    .ok_or_else(|| "--samples requires a positive integer".to_string())?
                    .parse::<usize>()
                    .map_err(|_| "--samples requires a positive integer".to_string())?;
                if sample_count == 0 {
                    return Err("--samples requires a positive integer".to_string());
                }
            }
            "--module-count" => {
                let count = arguments
                    .next()
                    .ok_or_else(|| "--module-count requires a fixture size".to_string())?
                    .parse::<usize>()
                    .map_err(|_| "--module-count requires a fixture size".to_string())?;
                if !MODULE_COUNTS.contains(&count) {
                    return Err("--module-count must be 10, 100, 1000, or 10000".to_string());
                }
                module_count = Some(count);
            }
            "--scope" => {
                scope = Some(ClosureScope::parse(
                    arguments
                        .next()
                        .ok_or_else(|| "--scope requires a closure scope".to_string())?
                        .as_str(),
                )?);
            }
            "--help" | "-h" => {
                println!(
                    "Usage: cargo run --release -p galfus-benchmark --bin workspace_scaling_baseline -- [--samples N] [--module-count 10|100|1000|10000] [--scope small|all-reachable]"
                );
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument {argument}")),
        }
    }
    Ok(Options {
        sample_count,
        module_count,
        scope,
    })
}

fn run(options: Options) -> Result<(), String> {
    let command = command(&options);
    let module_counts = options
        .module_count
        .map_or_else(|| MODULE_COUNTS.to_vec(), |count| vec![count]);
    let scopes = options
        .scope
        .map_or_else(|| ClosureScope::ALL.to_vec(), |scope| vec![scope]);
    let repository_root = repository_root()?;
    let mut report = ScalingReport {
        schema_version: 1,
        generated_unix_seconds: unix_timestamp()?,
        command,
        sample_count: options.sample_count,
        fixtures: Vec::new(),
    };
    for source_modules in module_counts {
        for scope in &scopes {
            let fixture = build_fixture(repository_root.as_path(), source_modules, *scope)?;
            let mut samples = Vec::with_capacity(options.sample_count);
            for sample_index in 0..options.sample_count {
                print!(
                    "Measuring {} source modules / {} closure sample {}/{}... ",
                    source_modules,
                    scope.as_str(),
                    sample_index + 1,
                    options.sample_count
                );
                let sample = measure_fixture(fixture.path.as_path())?;
                println!("{} us", sample.pipeline_us);
                samples.push(sample);
            }
            report.fixtures.push(ScalingFixtureReport {
                fixture: fixture.name,
                source_modules,
                entry_closure_modules: fixture.entry_closure_modules,
                scope: scope.as_str().to_string(),
                samples,
            });
            write_report(&report, repository_root.as_path())?;
        }
    }
    let (json_path, markdown_path) = write_report(&report, repository_root.as_path())?;
    println!(
        "Workspace scaling baseline written to {}",
        json_path.display()
    );
    println!(
        "Human-readable summary written to {}",
        markdown_path.display()
    );
    Ok(())
}

fn command(options: &Options) -> String {
    let mut command = format!(
        "cargo run --release -p galfus-benchmark --bin workspace_scaling_baseline -- --samples {}",
        options.sample_count
    );
    if let Some(module_count) = options.module_count {
        command.push_str(format!(" --module-count {module_count}").as_str());
    }
    if let Some(scope) = options.scope {
        command.push_str(format!(" --scope {}", scope.as_str()).as_str());
    }
    command
}

fn measure_fixture(root: &Path) -> Result<ScalingSample, String> {
    let pipeline_started = Instant::now();
    let load_started = Instant::now();
    let mut workspace = load_workspace(root)?;
    let source_load_us = elapsed_us(load_started);
    let user_module_ids = workspace
        .source_state
        .store
        .iter()
        .filter(|entry| entry.origin == ModuleOrigin::User)
        .map(|entry| entry.module_id)
        .collect::<HashSet<_>>();

    let check_started = Instant::now();
    if !workspace.check().is_valid {
        return Err(format!("fixture {} failed workspace check", root.display()));
    }
    let interface_check_us = elapsed_us(check_started);
    let interface_checked_modules = workspace
        .module_catalog()
        .ok_or_else(|| format!("fixture {} has no module catalog", root.display()))?
        .iter()
        .filter(|module| user_module_ids.contains(&module.module_id()))
        .count();

    let runtime_start_started = Instant::now();
    let capabilities = Some(default_providers(workspace.package_metadata()));
    let driver: Rc<dyn ExecutionDriver> = Rc::new(NativeDriver::new());
    let mut execution = workspace
        .start_execution(&[], capabilities, driver)
        .map_err(|error| format!("fixture {} failed runtime start: {error:?}", root.display()))?;
    let runtime_start_us = elapsed_us(runtime_start_started);

    let entry_completion_started = Instant::now();
    let exit_code = execution
        .run_sync_to_completion()
        .map_err(|error| format!("fixture {} failed execution: {error}", root.display()))?;
    let entry_completion_us = elapsed_us(entry_completion_started);
    let bytecode_produced_modules = execution
        .loaded_module_ids()
        .iter()
        .filter(|module_id| user_module_ids.contains(module_id))
        .count();

    Ok(ScalingSample {
        source_load_us,
        interface_check_us,
        runtime_start_us,
        entry_completion_us,
        pipeline_us: elapsed_us(pipeline_started),
        source_loaded_modules: user_module_ids.len(),
        interface_checked_modules,
        bytecode_produced_modules,
        exit_code,
    })
}

fn load_workspace(root: &Path) -> Result<Workspace, String> {
    let manifest_source = fs::read_to_string(root.join("galfus.toml"))
        .map_err(|error| format!("could not read fixture manifest: {error}"))?;
    let manifest = toml::from_str::<WorkspaceManifest>(manifest_source.as_str())
        .map_err(|error| format!("could not parse fixture manifest: {error}"))?;
    let mut workspace = Workspace::new();
    workspace.set_catalog(Arc::new(native_catalog()));
    if let LoadResult::Diagnostics(diagnostics) = workspace
        .load_manifest(manifest)
        .map_err(|error| format!("could not load fixture manifest: {error:?}"))?
    {
        return Err(format!("fixture manifest diagnostics: {diagnostics:?}"));
    }
    load_source_directory(&mut workspace, root, root)?;
    Ok(workspace)
}

fn load_source_directory(
    workspace: &mut Workspace,
    workspace_root: &Path,
    directory: &Path,
) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("could not read fixture source directory: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not enumerate fixture source directory: {error}"))?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|error| format!("could not read fixture source type: {error}"))?
            .is_dir()
        {
            load_source_directory(workspace, workspace_root, path.as_path())?;
            continue;
        }
        if path.extension().is_none_or(|extension| extension != "gfs") {
            continue;
        }
        let source = fs::read(path.as_path())
            .map_err(|error| format!("could not read fixture source: {error}"))?;
        let module_path = path
            .strip_prefix(workspace_root)
            .map_err(|error| format!("fixture source escaped root: {error}"))?
            .to_string_lossy()
            .replace('\\', "/");
        if let LoadResult::Diagnostics(diagnostics) = workspace
            .load_module(module_path.as_str(), source.as_slice())
            .map_err(|error| format!("could not load fixture source: {error:?}"))?
        {
            return Err(format!("fixture source diagnostics: {diagnostics:?}"));
        }
    }
    Ok(())
}

fn repository_root() -> Result<std::path::PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .ok_or_else(|| "could not determine repository root".to_string())
}

fn elapsed_us(started: Instant) -> u64 {
    started.elapsed().as_micros() as u64
}
