use cli_table::{Cell, Style, Table, format::Justify};
use regex::Regex;
use serde::Serialize;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};

const SAMPLE_COUNT: usize = 7;
const MEMORY_POLL_INTERVAL: Duration = Duration::from_millis(1);
const JAVA_OUTPUT_DIR: &str = "./target/release/benchmark-java";
const SERVER_REQUEST_COUNT: usize = 10_000;
const SERVER_CONCURRENCY: usize = 32;
const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_START_ATTEMPTS: usize = 3;
const SERVER_PORT_MIN: u16 = 18_080;
const SERVER_PORT_MAX: u16 = 18_180;

struct BenchmarkCase {
    name: &'static str,
    galfus_source: &'static str,
    standalone_output: &'static str,
    java_class: &'static str,
    targets: &'static [(&'static str, &'static [&'static str])],
}

const BENCHMARK_CASES: &[BenchmarkCase] = &[
    BenchmarkCase {
        name: "Fibonacci 35",
        galfus_source: "benchmark/fib.gfs",
        standalone_output: "./target/release/fib_standalone",
        java_class: "Fib",
        targets: &[
            ("JavaScript (Bun)", &["benchmark/fib.js"]),
            ("JavaScript (QuickJS)", &["qjs", "benchmark/fib.js"]),
            ("Lua JIT", &["luajit", "benchmark/fib.lua"]),
            ("Lua 5.4", &["lua", "benchmark/fib.lua"]),
            ("Python 3", &["python3", "benchmark/fib.py"]),
        ],
    },
    BenchmarkCase {
        name: "Matrix 4x4 (i64)",
        galfus_source: "benchmark/matrix4.gfs",
        standalone_output: "./target/release/matrix4_standalone",
        java_class: "Matrix4",
        targets: &[
            ("JavaScript (Bun)", &["benchmark/matrix4.js"]),
            ("JavaScript (QuickJS)", &["qjs", "benchmark/matrix4.js"]),
            ("Lua JIT", &["luajit", "benchmark/matrix4.lua"]),
            ("Lua 5.4", &["lua", "benchmark/matrix4.lua"]),
            ("Python 3", &["python3", "benchmark/matrix4.py"]),
        ],
    },
    BenchmarkCase {
        name: "Four CPU Tasks",
        galfus_source: "benchmark/tasks.gfs",
        standalone_output: "./target/release/tasks_standalone",
        java_class: "Tasks",
        targets: &[
            ("JavaScript (Bun)", &["benchmark/tasks.js"]),
            ("Python 3", &["python3", "benchmark/tasks.py"]),
        ],
    },
];

const SERVER_BENCHMARK: BenchmarkCase = BenchmarkCase {
    name: "HTTP/1.1 Request Overload",
    galfus_source: "benchmark/server.gfs",
    standalone_output: "./target/release/server_standalone",
    java_class: "Server",
    targets: &[
        ("JavaScript (Bun)", &["benchmark/server.js", "{port}"]),
        ("Python 3", &["python3", "benchmark/server.py", "{port}"]),
    ],
};

#[derive(Debug, Serialize)]
struct BenchmarkResult {
    benchmark: String,
    language: String,
    script_time_ms: u64,
    total_time_ms: u64,
    cold_start_ms: u64,
    result: String,
    peak_rss_mb: f64,
    peak_virtual_mb: f64,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkSample {
    script_time_ms: u64,
    total_time_ms: u64,
    result: String,
    peak_rss_bytes: u64,
    peak_virtual_bytes: u64,
}

#[derive(Debug, Serialize)]
struct RawBenchmarkRun {
    benchmark: String,
    language: String,
    command: Vec<String>,
    samples: Vec<BenchmarkSample>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct CommandVersion {
    command: String,
    version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkEnvironment {
    sample_count: usize,
    reuse_release_binaries: bool,
    commands: Vec<CommandVersion>,
}

#[derive(Debug, Serialize)]
struct RawBenchmarkReport<'a> {
    environment: BenchmarkEnvironment,
    runs: &'a [RawBenchmarkRun],
}

struct RunningServer {
    child: Child,
    monitor: JoinHandle<(u64, u64)>,
    stop_tx: mpsc::Sender<()>,
    started: Instant,
    command: String,
}

struct ServerMetrics {
    total_time_ms: u64,
    peak_rss_bytes: u64,
    peak_virtual_bytes: u64,
    stderr: String,
}

#[derive(Debug, Serialize)]
struct BenchmarkSummaryReport<'a> {
    environment: BenchmarkEnvironment,
    summaries: &'a [BenchmarkResult],
}

fn reuse_release_binaries() -> bool {
    std::env::args()
        .skip(1)
        .any(|argument| argument == "--reuse-release")
}

fn http_only() -> bool {
    std::env::args()
        .skip(1)
        .any(|argument| argument == "--http-only")
}

fn main() {
    let reuse_release = reuse_release_binaries();
    let http_only = http_only();
    if reuse_release {
        println!("Reusing existing release binaries.");
    } else {
        println!("Compiling Galfus Engine...");
        let status = Command::new("cargo")
            .args(["build", "--workspace", "--profile", "release"])
            .status()
            .expect("Failed to compile Galfus");
        if !status.success() {
            eprintln!("Compilation failed!");
            return;
        }
    }

    if !http_only {
        for benchmark in BENCHMARK_CASES {
            if !compile_standalone(benchmark, reuse_release) {
                return;
            }
        }
    }
    if !compile_standalone(&SERVER_BENCHMARK, reuse_release) {
        return;
    }

    let re_result = Regex::new(r"RESULT=([^\r\n]+)").unwrap();
    let re_time = Regex::new(r"TIME_MS=(\d+)").unwrap();
    let bun_bin = bun_binary();
    let qjs_bin = qjs_binary();
    let java_available = prepare_java_benchmarks();

    let mut results = Vec::new();
    let mut raw_runs = Vec::new();
    println!("\nRunning {SAMPLE_COUNT} samples per benchmark...\n");

    if !http_only {
        for benchmark in BENCHMARK_CASES {
            let mut targets = benchmark
                .targets
                .iter()
                .map(|(name, command)| {
                    let mut command = command
                        .iter()
                        .map(|arg| (*arg).to_string())
                        .collect::<Vec<_>>();
                    if *name == "JavaScript (Bun)" {
                        command.insert(0, bun_bin.clone());
                    }
                    if *name == "JavaScript (QuickJS)" {
                        command[0] = qjs_bin.clone();
                    }
                    (*name, command)
                })
                .collect::<Vec<_>>();
            targets.push((
                "Galfus (Workspace)",
                vec![
                    "./target/release/galfus-cli".to_string(),
                    "run".to_string(),
                    benchmark.galfus_source.to_string(),
                ],
            ));
            targets.push((
                "Galfus (Host)",
                vec![benchmark.standalone_output.to_string()],
            ));
            if java_available {
                targets.push((
                    "Java",
                    vec![
                        "java".to_string(),
                        "-cp".to_string(),
                        JAVA_OUTPUT_DIR.to_string(),
                        benchmark.java_class.to_string(),
                    ],
                ));
            }

            for (name, command) in targets {
                print!("Testing {} / {name}... ", benchmark.name);
                std::io::stdout().flush().unwrap();

                let mut samples = Vec::with_capacity(SAMPLE_COUNT);
                let mut failure = None;
                for _ in 0..SAMPLE_COUNT {
                    match run_sample(&command, &re_result, &re_time) {
                        Ok(sample) => {
                            samples.push(sample);
                            print!(".");
                        }
                        Err(error) => {
                            println!(" Failed: {error}");
                            failure = Some(error);
                            break;
                        }
                    }
                    std::io::stdout().flush().unwrap();
                }

                raw_runs.push(RawBenchmarkRun {
                    benchmark: benchmark.name.to_string(),
                    language: name.to_string(),
                    command: command.clone(),
                    samples: samples.clone(),
                    error: failure,
                });
                if let Err(error) =
                    write_reports(reuse_release, raw_runs.as_slice(), results.as_slice())
                {
                    eprintln!("Could not persist benchmark reports: {error}");
                }

                let Some(result) = summarize(benchmark.name, name, samples) else {
                    println!(" Skipped");
                    continue;
                };
                println!(
                    " {}ms median (total {}ms)",
                    result.script_time_ms, result.total_time_ms
                );
                results.push(result);
                if let Err(error) =
                    write_reports(reuse_release, raw_runs.as_slice(), results.as_slice())
                {
                    eprintln!("Could not persist benchmark reports: {error}");
                }
            }
        }
    }

    run_server_benchmark(
        &SERVER_BENCHMARK,
        reuse_release,
        &bun_bin,
        java_available,
        &mut results,
        &mut raw_runs,
    );

    results.sort_by(|left, right| {
        left.benchmark
            .cmp(&right.benchmark)
            .then(left.script_time_ms.cmp(&right.script_time_ms))
    });
    print_results(results.as_slice());
    if let Err(error) = write_reports(reuse_release, raw_runs.as_slice(), results.as_slice()) {
        eprintln!("Could not persist benchmark reports: {error}");
    }
}

fn compile_standalone(benchmark: &BenchmarkCase, reuse_release: bool) -> bool {
    if reuse_release && std::path::Path::new(benchmark.standalone_output).exists() {
        return true;
    }
    println!("Compiling standalone Galfus ({})...", benchmark.name);
    let status = Command::new("./target/release/galfus-cli")
        .args([
            "compile",
            "--local-host",
            "./target/release/galfus-host-native",
            "-t",
            "native",
            "-o",
            benchmark.standalone_output,
            benchmark.galfus_source,
        ])
        .env("GALFUS_CACHE_DIR", ".tmp/galfus-cache")
        .status()
        .expect("Failed to compile standalone host");
    if !status.success() {
        eprintln!("Standalone compilation failed for {}!", benchmark.name);
        return false;
    }
    true
}

fn prepare_java_benchmarks() -> bool {
    if let Err(error) = std::fs::create_dir_all(JAVA_OUTPUT_DIR) {
        eprintln!("Skipping Java: could not create output directory: {error}");
        return false;
    }

    let status = match Command::new("javac")
        .args([
            "-d",
            JAVA_OUTPUT_DIR,
            "benchmark/Fib.java",
            "benchmark/Matrix4.java",
            "benchmark/Tasks.java",
            "benchmark/Server.java",
        ])
        .status()
    {
        Ok(status) => status,
        Err(error) => {
            eprintln!("Skipping Java: javac is unavailable: {error}");
            return false;
        }
    };
    if !status.success() {
        eprintln!("Skipping Java: javac failed with {status}");
        return false;
    }
    true
}

fn bun_binary() -> String {
    std::env::var("BUN_INSTALL")
        .map(|value| format!("{value}/bin/bun"))
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .ok()
                .map(|home| format!("{home}/.bun/bin/bun"))
                .filter(|path| std::path::Path::new(path).exists())
                .unwrap_or_else(|| "bun".to_string())
        })
}

fn qjs_binary() -> String {
    std::env::var("QJS_BIN").unwrap_or_else(|_| {
        std::env::var("HOME")
            .ok()
            .map(|home| format!("{home}/Scripts/qjs"))
            .filter(|path| std::path::Path::new(path).exists())
            .unwrap_or_else(|| "qjs".to_string())
    })
}

fn write_reports(
    reuse_release: bool,
    raw_runs: &[RawBenchmarkRun],
    summaries: &[BenchmarkResult],
) -> Result<(), String> {
    let output_dir = std::path::Path::new(".tmp/benchmark");
    std::fs::create_dir_all(output_dir).map_err(|error| error.to_string())?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    let environment = BenchmarkEnvironment {
        sample_count: SAMPLE_COUNT,
        reuse_release_binaries: reuse_release,
        commands: [
            "rustc".to_string(),
            "cargo".to_string(),
            qjs_binary(),
            bun_binary(),
            "java".to_string(),
            "python3".to_string(),
            "lua".to_string(),
            "luajit".to_string(),
        ]
        .iter()
        .map(|command| command_version(command))
        .collect(),
    };
    write_json(
        output_dir.join(format!("{timestamp}-raw.json")),
        &RawBenchmarkReport {
            environment: environment.clone(),
            runs: raw_runs,
        },
    )?;
    write_json(
        output_dir.join(format!("{timestamp}-summary.json")),
        &BenchmarkSummaryReport {
            environment,
            summaries,
        },
    )
}

fn command_version(command: &str) -> CommandVersion {
    let version = Command::new(command)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|version| version.lines().next().unwrap_or_default().to_string());
    CommandVersion {
        command: command.to_string(),
        version,
    }
}

fn write_json(path: std::path::PathBuf, value: &impl Serialize) -> Result<(), String> {
    let contents = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    std::fs::write(path, contents).map_err(|error| error.to_string())
}

fn run_sample(
    command: &[String],
    re_result: &Regex,
    re_time: &Regex,
) -> Result<BenchmarkSample, String> {
    let mut process = Command::new(&command[0]);
    process
        .args(&command[1..])
        .env("GALFUS_CACHE_DIR", ".tmp/galfus-cache")
        .stdout(Stdio::piped());

    let started = Instant::now();
    let child = process.spawn().map_err(|error| error.to_string())?;
    let pid = Pid::from_u32(child.id());
    let (stop_tx, stop_rx) = std::sync::mpsc::channel();
    let monitor = thread::spawn(move || monitor_memory(pid, stop_rx));

    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    let total_time_ms = started.elapsed().as_millis() as u64;
    let _ = stop_tx.send(());
    let (peak_rss_bytes, peak_virtual_bytes) = monitor.join().unwrap();
    if !output.status.success() {
        return Err(format!("process exited with {}", output.status));
    }

    let output = String::from_utf8_lossy(&output.stdout);
    let result = capture_text(re_result, &output, "RESULT")?;
    let script_time_ms = capture_text(re_time, &output, "TIME_MS")?
        .parse()
        .map_err(|error| format!("invalid TIME_MS output: {error}"))?;
    Ok(BenchmarkSample {
        script_time_ms,
        total_time_ms,
        result,
        peak_rss_bytes,
        peak_virtual_bytes,
    })
}

fn run_server_benchmark(
    benchmark: &BenchmarkCase,
    reuse_release: bool,
    bun_bin: &str,
    java_available: bool,
    results: &mut Vec<BenchmarkResult>,
    raw_runs: &mut Vec<RawBenchmarkRun>,
) {
    let mut targets = vec![
        (
            "Galfus (Workspace)",
            vec![
                "./target/release/galfus-cli".to_string(),
                "run".to_string(),
                benchmark.galfus_source.to_string(),
                "--".to_string(),
                "{port}".to_string(),
            ],
        ),
        (
            "Galfus (Host)",
            vec![
                benchmark.standalone_output.to_string(),
                "{port}".to_string(),
            ],
        ),
    ];
    targets.extend(
        benchmark
            .targets
            .iter()
            .map(|(name, command)| {
                let mut command = command
                    .iter()
                    .map(|arg| (*arg).to_string())
                    .collect::<Vec<_>>();
                if *name == "JavaScript (Bun)" {
                    command.insert(0, bun_bin.to_string());
                }
                (*name, command)
            })
            .collect::<Vec<_>>(),
    );
    if java_available {
        targets.push((
            "Java",
            vec![
                "java".to_string(),
                "--add-modules".to_string(),
                "jdk.httpserver".to_string(),
                "-cp".to_string(),
                JAVA_OUTPUT_DIR.to_string(),
                benchmark.java_class.to_string(),
                "{port}".to_string(),
            ],
        ));
    }

    for (name, command) in targets {
        print!("Testing {} / {name}... ", benchmark.name);
        std::io::stdout().flush().unwrap();

        let mut samples = Vec::with_capacity(SAMPLE_COUNT);
        let mut failure = None;
        for _ in 0..SAMPLE_COUNT {
            match run_server_sample(&command) {
                Ok(sample) => {
                    samples.push(sample);
                    print!(".");
                }
                Err(error) => {
                    println!(" Failed: {error}");
                    failure = Some(error);
                    break;
                }
            }
            std::io::stdout().flush().unwrap();
        }

        raw_runs.push(RawBenchmarkRun {
            benchmark: benchmark.name.to_string(),
            language: name.to_string(),
            command: command.clone(),
            samples: samples.clone(),
            error: failure,
        });
        if let Err(error) = write_reports(reuse_release, raw_runs.as_slice(), results.as_slice()) {
            eprintln!("Could not persist benchmark reports: {error}");
        }

        let Some(result) = summarize(benchmark.name, name, samples) else {
            println!(" Skipped");
            continue;
        };
        println!(
            " {}ms median (total {}ms)",
            result.script_time_ms, result.total_time_ms
        );
        results.push(result);
        if let Err(error) = write_reports(reuse_release, raw_runs.as_slice(), results.as_slice()) {
            eprintln!("Could not persist benchmark reports: {error}");
        }
    }
}

fn run_server_sample(command: &[String]) -> Result<BenchmarkSample, String> {
    let (port, server) = start_server_with_retry(command)?;
    let load_started = Instant::now();
    let result = run_http_load(port);
    let script_time_ms = load_started.elapsed().as_millis() as u64;
    let metrics = server.stop()?;
    result?;

    Ok(BenchmarkSample {
        script_time_ms,
        total_time_ms: metrics.total_time_ms,
        result: format!("{SERVER_REQUEST_COUNT} requests at {SERVER_CONCURRENCY} concurrency"),
        peak_rss_bytes: metrics.peak_rss_bytes,
        peak_virtual_bytes: metrics.peak_virtual_bytes,
    })
}

fn start_server_with_retry(command: &[String]) -> Result<(u16, RunningServer), String> {
    let mut errors = Vec::with_capacity(SERVER_START_ATTEMPTS);
    for _ in 0..SERVER_START_ATTEMPTS {
        let port = available_server_port()?;
        match start_server(command, port) {
            Ok(server) => return Ok((port, server)),
            Err(error) => errors.push(error),
        }
    }
    Err(format!(
        "server did not start after {SERVER_START_ATTEMPTS} attempts:\n{}",
        errors.join("\n")
    ))
}

fn start_server(command: &[String], port: u16) -> Result<RunningServer, String> {
    let started = Instant::now();
    let command = command
        .iter()
        .map(|argument| argument.replace("{port}", &port.to_string()))
        .collect::<Vec<_>>();
    let child = Command::new(&command[0])
        .args(&command[1..])
        .env("GALFUS_CACHE_DIR", ".tmp/galfus-cache")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let pid = Pid::from_u32(child.id());
    let (stop_tx, stop_rx) = mpsc::channel();
    let monitor = thread::spawn(move || monitor_memory(pid, stop_rx));
    let server = RunningServer {
        child,
        monitor,
        stop_tx,
        started,
        command: command.join(" "),
    };
    if let Err(error) = wait_for_server(port) {
        let command = server.command.clone();
        return match server.stop() {
            Ok(metrics) if metrics.stderr.is_empty() => {
                Err(format!("{error} while starting `{command}`"))
            }
            Ok(metrics) => Err(format!(
                "{error} while starting `{command}`:\n{}",
                metrics.stderr.trim()
            )),
            Err(stop_error) => Err(format!("{error}; could not stop server: {stop_error}")),
        };
    }
    Ok(server)
}

impl RunningServer {
    fn stop(mut self) -> Result<ServerMetrics, String> {
        let _ = self.child.kill();
        self.child.wait().map_err(|error| error.to_string())?;
        let mut stderr = String::new();
        if let Some(mut child_stderr) = self.child.stderr.take() {
            child_stderr
                .read_to_string(&mut stderr)
                .map_err(|error| error.to_string())?;
        }
        let _ = self.stop_tx.send(());
        let (peak_rss_bytes, peak_virtual_bytes) = self
            .monitor
            .join()
            .map_err(|_| "server memory monitor panicked".to_string())?;
        Ok(ServerMetrics {
            total_time_ms: self.started.elapsed().as_millis() as u64,
            peak_rss_bytes,
            peak_virtual_bytes,
            stderr,
        })
    }
}

fn available_server_port() -> Result<u16, String> {
    for port in SERVER_PORT_MIN..=SERVER_PORT_MAX {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }
    Err(format!(
        "no available server port in {SERVER_PORT_MIN}..={SERVER_PORT_MAX}"
    ))
}

fn wait_for_server(port: u16) -> Result<(), String> {
    let deadline = Instant::now() + SERVER_READY_TIMEOUT;
    loop {
        if send_http_request(port).is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "server did not accept requests within {SERVER_READY_TIMEOUT:?}"
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn run_http_load(port: u16) -> Result<(), String> {
    let next_request = Arc::new(AtomicUsize::new(0));
    let (error_tx, error_rx) = mpsc::channel();
    let mut workers = Vec::with_capacity(SERVER_CONCURRENCY);

    for _ in 0..SERVER_CONCURRENCY {
        let next_request = Arc::clone(&next_request);
        let error_tx = error_tx.clone();
        workers.push(thread::spawn(move || {
            loop {
                if next_request.fetch_add(1, Ordering::Relaxed) >= SERVER_REQUEST_COUNT {
                    return;
                }
                if let Err(error) = send_http_request(port) {
                    let _ = error_tx.send(error);
                    return;
                }
            }
        }));
    }
    drop(error_tx);

    for worker in workers {
        worker
            .join()
            .map_err(|_| "HTTP load worker panicked".to_string())?;
    }
    error_rx.try_recv().map_or(Ok(()), Err)
}

fn send_http_request(port: u16) -> Result<(), String> {
    let mut stream =
        TcpStream::connect(("127.0.0.1", port)).map_err(|error| format!("connect: {error}"))?;
    stream
        .set_nodelay(true)
        .map_err(|error| format!("set TCP_NODELAY: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("set read timeout: {error}"))?;
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .map_err(|error| format!("write request: {error}"))?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| format!("read response: {error}"))?;
    let response = String::from_utf8_lossy(&response);
    let status_line = response.lines().next().unwrap_or("empty response");
    if !status_line.starts_with("HTTP/") || status_line.split_whitespace().nth(1) != Some("200") {
        return Err(format!("expected HTTP 200, got {}", status_line));
    }
    Ok(())
}

fn monitor_memory(pid: Pid, stop_rx: std::sync::mpsc::Receiver<()>) -> (u64, u64) {
    let mut system = System::new();
    let mut peak_rss_bytes = 0;
    let mut peak_virtual_bytes = 0;
    loop {
        system.refresh_processes();
        if let Some(process) = system.process(pid) {
            peak_rss_bytes = peak_rss_bytes.max(process.memory());
            peak_virtual_bytes = peak_virtual_bytes.max(process.virtual_memory());
        }
        if stop_rx.try_recv().is_ok() {
            return (peak_rss_bytes, peak_virtual_bytes);
        }
        thread::sleep(MEMORY_POLL_INTERVAL);
    }
}

fn capture_text(regex: &Regex, output: &str, label: &str) -> Result<String, String> {
    Ok(regex
        .captures(output)
        .and_then(|captures| captures.get(1))
        .ok_or_else(|| format!("missing {label} output"))?
        .as_str()
        .to_string())
}

fn summarize(
    benchmark: &str,
    language: &str,
    samples: Vec<BenchmarkSample>,
) -> Option<BenchmarkResult> {
    if samples.len() != SAMPLE_COUNT || samples.iter().any(|sample| sample.result.is_empty()) {
        return None;
    }
    let script_time_ms = median(samples.iter().map(|sample| sample.script_time_ms).collect());
    let total_time_ms = median(samples.iter().map(|sample| sample.total_time_ms).collect());
    let result = samples[0].result.clone();
    let peak_rss_bytes = samples
        .iter()
        .map(|sample| sample.peak_rss_bytes)
        .max()
        .unwrap_or(0);
    let peak_virtual_bytes = samples
        .iter()
        .map(|sample| sample.peak_virtual_bytes)
        .max()
        .unwrap_or(0);

    Some(BenchmarkResult {
        benchmark: benchmark.to_string(),
        language: language.to_string(),
        script_time_ms,
        total_time_ms,
        cold_start_ms: total_time_ms.saturating_sub(script_time_ms),
        result,
        peak_rss_mb: bytes_to_mb(peak_rss_bytes),
        peak_virtual_mb: bytes_to_mb(peak_virtual_bytes),
    })
}

fn median(mut values: Vec<u64>) -> u64 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn bytes_to_mb(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}

fn print_results(results: &[BenchmarkResult]) {
    for benchmark_name in BENCHMARK_CASES
        .iter()
        .map(|benchmark| benchmark.name)
        .chain(std::iter::once(SERVER_BENCHMARK.name))
    {
        let rows = results
            .iter()
            .filter(|result| result.benchmark == benchmark_name)
            .enumerate()
            .map(|(index, result)| {
                vec![
                    index.cell(),
                    result.language.clone().cell(),
                    result.script_time_ms.cell().justify(Justify::Right),
                    result.cold_start_ms.cell().justify(Justify::Right),
                    result.total_time_ms.cell().justify(Justify::Right),
                    format!("{:.2}", result.peak_rss_mb)
                        .cell()
                        .justify(Justify::Right),
                    format!("{:.2}", result.peak_virtual_mb)
                        .cell()
                        .justify(Justify::Right),
                    result.result.clone().cell().justify(Justify::Right),
                ]
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            continue;
        }
        let table = rows.table().title(vec![
            "".cell().bold(true),
            "Language".cell().bold(true),
            "Script Median (ms)".cell().bold(true),
            "Cold Start (ms)".cell().bold(true),
            "Total Median (ms)".cell().bold(true),
            "Peak RSS (MB)".cell().bold(true),
            "Peak VMS (MB)".cell().bold(true),
            "Result".cell().bold(true),
        ]);

        println!("\n--- {benchmark_name} ---");
        if let Ok(table) = table.display() {
            println!("{table}");
        }
    }
}
