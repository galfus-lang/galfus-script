#[path = "module_resolver_baseline/report.rs"]
mod report;

use galfus_contract::{AdapterBindings, RuntimeCapabilities};
use galfus_host_native::driver::NativeDriver;
use galfus_host_native::native_catalog;
use galfus_host_native::providers::default_providers;
use galfus_runtime::Runtime;
use galfus_runtime::driver::ExecutionDriver;
use galfus_workspace::{LoadResult, Workspace, WorkspaceManifest};
use report::{
    BaselineReport, BaselineSample, ExecutionReport, FixtureReport, benchmark_environment,
    compare_with_phase_one, load_phase_one_baseline, statistics, unix_timestamp, write_report,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};

const DEFAULT_SAMPLE_COUNT: usize = 10;
const MEMORY_POLL_INTERVAL: Duration = Duration::from_millis(1);
const FIXTURE_NAMES: [&str; 2] = ["resolver-small", "resolver-stdlib"];
const PHASE_ONE_BASELINE_PATH: &str = ".tmp/benchmark/module-resolver-baseline-1789993467.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionMode {
    WorkspaceLazy,
    StandaloneEager,
}

impl ExecutionMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::WorkspaceLazy => "workspace-lazy",
            Self::StandaloneEager => "standalone-eager",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "workspace-lazy" => Ok(Self::WorkspaceLazy),
            "standalone-eager" => Ok(Self::StandaloneEager),
            _ => Err(format!(
                "unknown execution mode {value}; expected workspace-lazy or standalone-eager"
            )),
        }
    }
}

#[derive(Debug)]
struct Options {
    sample_count: usize,
    fixture: Option<String>,
    sample_fixture: Option<String>,
    sample_mode: Option<ExecutionMode>,
    phase_one_baseline: Option<PathBuf>,
}

fn main() -> ExitCode {
    match parse_options().and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("module resolver baseline failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_options() -> Result<Options, String> {
    let mut sample_count = DEFAULT_SAMPLE_COUNT;
    let mut fixture = None;
    let mut sample_fixture = None;
    let mut sample_mode = None;
    let mut phase_one_baseline = None;
    let mut arguments = env::args().skip(1);

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--samples" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--samples requires a positive integer".to_string())?;
                sample_count = value
                    .parse::<usize>()
                    .map_err(|_| "--samples requires a positive integer".to_string())?;
                if sample_count == 0 {
                    return Err("--samples requires a positive integer".to_string());
                }
            }
            "--fixture" => {
                fixture = Some(
                    arguments
                        .next()
                        .ok_or_else(|| "--fixture requires a fixture name".to_string())?,
                );
            }
            "--sample" => {
                sample_fixture = Some(
                    arguments
                        .next()
                        .ok_or_else(|| "--sample requires a fixture name".to_string())?,
                );
            }
            "--mode" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--mode requires an execution mode".to_string())?;
                sample_mode = Some(ExecutionMode::parse(value.as_str())?);
            }
            "--phase-one-baseline" => {
                phase_one_baseline =
                    Some(PathBuf::from(arguments.next().ok_or_else(|| {
                        "--phase-one-baseline requires a path".to_string()
                    })?));
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument {argument}")),
        }
    }

    if fixture.is_some() && sample_fixture.is_some() {
        return Err("--fixture and --sample cannot be used together".to_string());
    }
    if sample_mode.is_some() && sample_fixture.is_none() {
        return Err("--mode requires --sample".to_string());
    }

    Ok(Options {
        sample_count,
        fixture,
        sample_fixture,
        sample_mode,
        phase_one_baseline,
    })
}

fn print_usage() {
    println!(
        "Usage: cargo run --release -p galfus-benchmark --bin module_resolver_baseline -- [--samples N] [--fixture NAME] [--phase-one-baseline PATH]"
    );
}

fn run(options: Options) -> Result<(), String> {
    if let Some(fixture) = options.sample_fixture {
        validate_fixture_name(fixture.as_str())?;
        let sample = measure_sample(
            fixture.as_str(),
            options
                .sample_mode
                .unwrap_or(ExecutionMode::StandaloneEager),
        )?;
        println!(
            "{}",
            serde_json::to_string(&sample).map_err(|error| error.to_string())?
        );
        return Ok(());
    }

    let fixture_names = match options.fixture {
        Some(fixture) => {
            validate_fixture_name(fixture.as_str())?;
            vec![fixture]
        }
        None => FIXTURE_NAMES
            .iter()
            .map(|name| (*name).to_string())
            .collect(),
    };

    let phase_one_baseline_path = match options.phase_one_baseline {
        Some(path) => path,
        None => repository_root()?.join(PHASE_ONE_BASELINE_PATH),
    };
    let phase_one_baseline = load_phase_one_baseline(phase_one_baseline_path.as_path())?;
    let fixtures = fixture_names
        .iter()
        .map(|fixture| measure_fixture(fixture.as_str(), options.sample_count))
        .collect::<Result<Vec<_>, _>>()?;
    let fixture_argument =
        (fixture_names.len() == 1).then(|| format!(" --fixture {}", fixture_names[0]));
    let report = BaselineReport {
        schema_version: 2,
        generated_unix_seconds: unix_timestamp()?,
        command: format!(
            "cargo run --release -p galfus-benchmark --bin module_resolver_baseline -- --samples {}{}",
            options.sample_count,
            fixture_argument.unwrap_or_default()
        ),
        sample_count: options.sample_count,
        environment: benchmark_environment(),
        phase_one_comparison: compare_with_phase_one(
            phase_one_baseline_path.as_path(),
            &phase_one_baseline,
            fixtures.as_slice(),
        )?,
        fixtures,
    };
    let report_root = repository_root()?;
    let (json_path, markdown_path) = write_report(&report, report_root.as_path())?;
    println!(
        "Module resolver baseline written to {}",
        json_path.display()
    );
    println!(
        "Human-readable summary written to {}",
        markdown_path.display()
    );
    Ok(())
}

fn validate_fixture_name(fixture: &str) -> Result<(), String> {
    if FIXTURE_NAMES.contains(&fixture) {
        Ok(())
    } else {
        Err(format!(
            "unknown fixture {fixture}; expected one of {}",
            FIXTURE_NAMES.join(", ")
        ))
    }
}

fn measure_fixture(fixture: &str, sample_count: usize) -> Result<FixtureReport, String> {
    Ok(FixtureReport {
        fixture: fixture.to_string(),
        workspace_lazy: measure_execution_mode(
            fixture,
            sample_count,
            ExecutionMode::WorkspaceLazy,
        )?,
        standalone_eager: measure_execution_mode(
            fixture,
            sample_count,
            ExecutionMode::StandaloneEager,
        )?,
    })
}

fn measure_execution_mode(
    fixture: &str,
    sample_count: usize,
    mode: ExecutionMode,
) -> Result<ExecutionReport, String> {
    let mut samples = Vec::with_capacity(sample_count);
    for sample_index in 0..sample_count {
        print!(
            "Measuring {fixture} {} sample {}/{}... ",
            mode.as_str(),
            sample_index + 1,
            sample_count
        );
        let sample = measure_child_sample(fixture, mode)?;
        println!("{} us", sample.process_total_us);
        samples.push(sample);
    }
    Ok(ExecutionReport {
        metrics: statistics(&samples),
        samples,
    })
}

fn measure_child_sample(fixture: &str, mode: ExecutionMode) -> Result<BaselineSample, String> {
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let started = Instant::now();
    let child = Command::new(executable)
        .args(["--sample", fixture, "--mode", mode.as_str()])
        .current_dir(repository_root()?)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let pid = Pid::from_u32(child.id());
    let (stop_tx, stop_rx) = mpsc::channel();
    let monitor = thread::spawn(move || monitor_memory(pid, stop_rx));
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    let process_total_us = elapsed_us(started);
    let _ = stop_tx.send(());
    let peak_rss_bytes = monitor
        .join()
        .map_err(|_| "memory monitor panicked".to_string())?;

    if !output.status.success() {
        return Err(format!(
            "sample process exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let mut sample = serde_json::from_slice::<BaselineSample>(&output.stdout)
        .map_err(|error| format!("invalid sample output: {error}"))?;
    sample.process_total_us = process_total_us;
    sample.peak_rss_bytes = peak_rss_bytes;
    Ok(sample)
}

fn measure_sample(fixture: &str, mode: ExecutionMode) -> Result<BaselineSample, String> {
    match mode {
        ExecutionMode::WorkspaceLazy => measure_workspace_lazy_sample(fixture),
        ExecutionMode::StandaloneEager => measure_standalone_eager_sample(fixture),
    }
}

fn measure_workspace_lazy_sample(fixture: &str) -> Result<BaselineSample, String> {
    let pipeline_started = Instant::now();
    let discovery_started = Instant::now();
    let mut workspace = load_workspace(&fixture_path(fixture)?)?;
    let workspace_discovery_us = elapsed_us(discovery_started);

    let check_started = Instant::now();
    let is_valid = workspace.check().is_valid;
    let check_us = elapsed_us(check_started);
    if !is_valid {
        return Err(format!("fixture {fixture} failed workspace check"));
    }

    let cataloged_modules = workspace
        .module_catalog()
        .ok_or_else(|| format!("fixture {fixture} has no checked module catalog"))?
        .len();
    let runtime_start_started = Instant::now();
    let capabilities = Some(default_providers(workspace.package_metadata()));
    let driver: Rc<dyn ExecutionDriver> = Rc::new(NativeDriver::new());
    let mut execution = workspace
        .start_execution(&[], capabilities, driver)
        .map_err(|error| format!("fixture {fixture} failed lazy runtime start: {error:?}"))?;
    let runtime_start_us = elapsed_us(runtime_start_started);

    let entry_completion_started = Instant::now();
    let exit_code = execution
        .run_sync_to_completion()
        .map_err(|error| format!("fixture {fixture} failed lazy execution: {error}"))?;
    let entry_completion_us = elapsed_us(entry_completion_started);
    let materialized_modules = execution.loaded_module_ids().len();

    Ok(BaselineSample {
        workspace_discovery_us,
        check_us,
        compile_us: 0,
        package_encode_us: 0,
        runtime_start_us,
        entry_completion_us,
        pipeline_us: elapsed_us(pipeline_started),
        process_total_us: 0,
        package_bytes: 0,
        cataloged_modules,
        materialized_modules,
        decoded_chunks: 0,
        exit_code,
        peak_rss_bytes: 0,
    })
}

fn measure_standalone_eager_sample(fixture: &str) -> Result<BaselineSample, String> {
    let pipeline_started = Instant::now();
    let discovery_started = Instant::now();
    let mut workspace = load_workspace(&fixture_path(fixture)?)?;
    let workspace_discovery_us = elapsed_us(discovery_started);

    let check_started = Instant::now();
    let is_valid = workspace.check().is_valid;
    let check_us = elapsed_us(check_started);
    if !is_valid {
        return Err(format!("fixture {fixture} failed workspace check"));
    }

    let compile_started = Instant::now();
    let package = workspace
        .compile()
        .map_err(|error| format!("fixture {fixture} failed compilation: {error:?}"))?
        .package;
    let compile_us = elapsed_us(compile_started);
    let cataloged_modules = package.catalog().len();

    let package_encode_started = Instant::now();
    let package_bytes = package
        .to_bytecode()
        .map_err(|error| format!("fixture {fixture} failed package encoding: {error}"))?;
    let package_encode_us = elapsed_us(package_encode_started);

    let runtime_start_started = Instant::now();
    let capabilities = RuntimeCapabilities::builder()
        .with_providers(default_providers(package.metadata().clone()))
        .with_adapter_bindings(AdapterBindings::default())
        .build();
    let runtime = Runtime::new(package, capabilities);
    let driver: Rc<dyn ExecutionDriver> = Rc::new(NativeDriver::new());
    let mut execution = runtime
        .start(&[], driver)
        .map_err(|error| format!("fixture {fixture} failed runtime start: {error}"))?;
    let runtime_start_us = elapsed_us(runtime_start_started);

    let entry_completion_started = Instant::now();
    let exit_code = execution
        .run_sync_to_completion()
        .map_err(|error| format!("fixture {fixture} failed execution: {error}"))?;
    let entry_completion_us = elapsed_us(entry_completion_started);
    let materialized_modules = execution.loaded_module_ids().len();

    Ok(BaselineSample {
        workspace_discovery_us,
        check_us,
        compile_us,
        package_encode_us,
        runtime_start_us,
        entry_completion_us,
        pipeline_us: elapsed_us(pipeline_started),
        process_total_us: 0,
        package_bytes: package_bytes.len(),
        cataloged_modules,
        materialized_modules,
        decoded_chunks: materialized_modules,
        exit_code,
        peak_rss_bytes: 0,
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
        let file_type = entry
            .file_type()
            .map_err(|error| format!("could not read fixture source type: {error}"))?;
        if file_type.is_dir() {
            load_source_directory(workspace, workspace_root, path.as_path())?;
            continue;
        }
        if !file_type.is_file() || path.extension().is_none_or(|extension| extension != "gfs") {
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

fn fixture_path(fixture: &str) -> Result<PathBuf, String> {
    let path = repository_root()?
        .join("benchmark")
        .join("module-resolver")
        .join(fixture);
    if path.join("galfus.toml").is_file() {
        Ok(path)
    } else {
        Err(format!("fixture path does not exist: {}", path.display()))
    }
}

fn repository_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .ok_or_else(|| "could not determine repository root".to_string())
}

fn monitor_memory(pid: Pid, stop_rx: mpsc::Receiver<()>) -> u64 {
    let mut system = System::new();
    let mut peak_rss_bytes = 0;
    loop {
        system.refresh_processes();
        if let Some(process) = system.process(pid) {
            peak_rss_bytes = peak_rss_bytes.max(process.memory());
        }
        if stop_rx.try_recv().is_ok() {
            return peak_rss_bytes;
        }
        thread::sleep(MEMORY_POLL_INTERVAL);
    }
}

fn elapsed_us(started: Instant) -> u64 {
    started.elapsed().as_micros() as u64
}
