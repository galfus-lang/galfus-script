use galfus_contract::{AdapterBindings, RuntimeCapabilities};
use galfus_host_native::driver::NativeDriver;
use galfus_host_native::native_catalog;
use galfus_host_native::providers::default_providers;
use galfus_runtime::Runtime;
use galfus_runtime::driver::ExecutionDriver;
use galfus_workspace::{LoadResult, Workspace, WorkspaceManifest};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{Pid, System};

const DEFAULT_SAMPLE_COUNT: usize = 10;
const MEMORY_POLL_INTERVAL: Duration = Duration::from_millis(1);
const FIXTURE_NAMES: [&str; 2] = ["resolver-small", "resolver-stdlib"];

#[derive(Debug)]
struct Options {
    sample_count: usize,
    fixture: Option<String>,
    sample_fixture: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BaselineSample {
    workspace_discovery_us: u64,
    check_us: u64,
    compile_us: u64,
    package_encode_us: u64,
    runtime_start_us: u64,
    entry_completion_us: u64,
    pipeline_us: u64,
    process_total_us: u64,
    package_bytes: usize,
    exit_code: i32,
    peak_rss_bytes: u64,
}

#[derive(Debug, Serialize)]
struct FixtureReport {
    fixture: String,
    samples: Vec<BaselineSample>,
    median: BaselineMedian,
}

#[derive(Debug, Serialize)]
struct BaselineMedian {
    workspace_discovery_us: u64,
    check_us: u64,
    compile_us: u64,
    package_encode_us: u64,
    runtime_start_us: u64,
    entry_completion_us: u64,
    pipeline_us: u64,
    process_total_us: u64,
    package_bytes: usize,
    peak_rss_bytes: u64,
}

#[derive(Debug, Serialize)]
struct BenchmarkEnvironment {
    git_revision: Option<String>,
    operating_system: String,
    architecture: String,
    cpu: Option<String>,
    rustc: Option<String>,
    cargo: Option<String>,
}

#[derive(Debug, Serialize)]
struct BaselineReport {
    schema_version: u8,
    generated_unix_seconds: u64,
    command: String,
    sample_count: usize,
    environment: BenchmarkEnvironment,
    fixtures: Vec<FixtureReport>,
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

    Ok(Options {
        sample_count,
        fixture,
        sample_fixture,
    })
}

fn print_usage() {
    println!(
        "Usage: cargo run --release -p galfus-benchmark --bin module_resolver_baseline -- [--samples N] [--fixture NAME]"
    );
}

fn run(options: Options) -> Result<(), String> {
    if let Some(fixture) = options.sample_fixture {
        validate_fixture_name(fixture.as_str())?;
        let sample = measure_sample(fixture.as_str())?;
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

    let report = BaselineReport {
        schema_version: 1,
        generated_unix_seconds: unix_timestamp()?,
        command: format!(
            "cargo run --release -p galfus-benchmark --bin module_resolver_baseline -- --samples {}",
            options.sample_count
        ),
        sample_count: options.sample_count,
        environment: benchmark_environment(),
        fixtures: fixture_names
            .iter()
            .map(|fixture| measure_fixture(fixture.as_str(), options.sample_count))
            .collect::<Result<Vec<_>, _>>()?,
    };
    let (json_path, markdown_path) = write_report(&report)?;
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
    let mut samples = Vec::with_capacity(sample_count);
    for sample_index in 0..sample_count {
        print!(
            "Measuring {fixture} sample {}/{}... ",
            sample_index + 1,
            sample_count
        );
        let sample = measure_child_sample(fixture)?;
        println!("{} us", sample.process_total_us);
        samples.push(sample);
    }
    Ok(FixtureReport {
        fixture: fixture.to_string(),
        median: median(&samples),
        samples,
    })
}

fn measure_child_sample(fixture: &str) -> Result<BaselineSample, String> {
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let started = Instant::now();
    let child = Command::new(executable)
        .args(["--sample", fixture])
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

fn measure_sample(fixture: &str) -> Result<BaselineSample, String> {
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

fn median(samples: &[BaselineSample]) -> BaselineMedian {
    BaselineMedian {
        workspace_discovery_us: median_by(samples, |sample| sample.workspace_discovery_us),
        check_us: median_by(samples, |sample| sample.check_us),
        compile_us: median_by(samples, |sample| sample.compile_us),
        package_encode_us: median_by(samples, |sample| sample.package_encode_us),
        runtime_start_us: median_by(samples, |sample| sample.runtime_start_us),
        entry_completion_us: median_by(samples, |sample| sample.entry_completion_us),
        pipeline_us: median_by(samples, |sample| sample.pipeline_us),
        process_total_us: median_by(samples, |sample| sample.process_total_us),
        package_bytes: median_by(samples, |sample| sample.package_bytes),
        peak_rss_bytes: median_by(samples, |sample| sample.peak_rss_bytes),
    }
}

fn median_by<T: Ord + Copy>(samples: &[BaselineSample], value: impl Fn(&BaselineSample) -> T) -> T {
    let mut values = samples.iter().map(value).collect::<Vec<_>>();
    values.sort_unstable();
    values[values.len() / 2]
}

fn write_report(report: &BaselineReport) -> Result<(PathBuf, PathBuf), String> {
    let output_dir = repository_root()?.join(".tmp").join("benchmark");
    fs::create_dir_all(output_dir.as_path()).map_err(|error| error.to_string())?;
    let prefix = format!("module-resolver-baseline-{}", report.generated_unix_seconds);
    let json_path = output_dir.join(format!("{prefix}.json"));
    let markdown_path = output_dir.join(format!("{prefix}.md"));
    let json = serde_json::to_vec_pretty(report).map_err(|error| error.to_string())?;
    fs::write(json_path.as_path(), json).map_err(|error| error.to_string())?;
    fs::write(markdown_path.as_path(), markdown_report(report))
        .map_err(|error| error.to_string())?;
    Ok((json_path, markdown_path))
}

fn markdown_report(report: &BaselineReport) -> String {
    let mut output = String::from("# Module resolver baseline\n\n");
    output.push_str(&format!("Command: {}\n\n", report.command));
    output.push_str(&format!("Samples per fixture: {}\n\n", report.sample_count));
    output.push_str("All durations are medians in milliseconds. RSS is the maximum observed child-process resident memory.\n\n");
    output.push_str("| Fixture | Discovery | Check | Compile | Encode | Runtime start | Entry completion | Pipeline | Process total | Package bytes | Peak RSS bytes |\n");
    output.push_str(
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    );
    for fixture in &report.fixtures {
        let median = &fixture.median;
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            fixture.fixture,
            milliseconds(median.workspace_discovery_us),
            milliseconds(median.check_us),
            milliseconds(median.compile_us),
            milliseconds(median.package_encode_us),
            milliseconds(median.runtime_start_us),
            milliseconds(median.entry_completion_us),
            milliseconds(median.pipeline_us),
            milliseconds(median.process_total_us),
            median.package_bytes,
            median.peak_rss_bytes,
        ));
    }
    output.push_str("\n## Environment\n\n");
    output.push_str(&format!(
        "- Git revision: {}\n",
        report
            .environment
            .git_revision
            .as_deref()
            .unwrap_or("unavailable")
    ));
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

fn milliseconds(microseconds: u64) -> String {
    format!("{:.3}", microseconds as f64 / 1_000.0)
}

fn benchmark_environment() -> BenchmarkEnvironment {
    let system = System::new_all();
    BenchmarkEnvironment {
        git_revision: command_output("git", &["rev-parse", "HEAD"]),
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

fn unix_timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())
        .map(|duration| duration.as_secs())
}

fn elapsed_us(started: Instant) -> u64 {
    started.elapsed().as_micros() as u64
}
