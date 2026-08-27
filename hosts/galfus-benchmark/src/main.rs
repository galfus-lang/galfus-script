use cli_table::{Cell, Style, Table, format::Justify};
use regex::Regex;
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};

const SAMPLE_COUNT: usize = 7;
const MEMORY_POLL_INTERVAL: Duration = Duration::from_millis(1);
const JAVA_OUTPUT_DIR: &str = "./target/release/benchmark-java";

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

#[derive(Debug)]
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

#[derive(Debug)]
struct BenchmarkSample {
    script_time_ms: u64,
    total_time_ms: u64,
    result: String,
    peak_rss_bytes: u64,
    peak_virtual_bytes: u64,
}

fn main() {
    println!("Compiling Galfus Engine...");
    let status = Command::new("cargo")
        .args(["build", "--workspace", "--profile", "release"])
        .status()
        .expect("Failed to compile Galfus");
    if !status.success() {
        eprintln!("Compilation failed!");
        return;
    }

    for benchmark in BENCHMARK_CASES {
        if !compile_standalone(benchmark) {
            return;
        }
    }

    let re_result = Regex::new(r"RESULT=([^\r\n]+)").unwrap();
    let re_time = Regex::new(r"TIME_MS=(\d+)").unwrap();
    let bun_bin = bun_binary();
    let java_available = prepare_java_benchmarks();

    let mut results = Vec::new();
    println!("\nRunning {SAMPLE_COUNT} cold-process samples per benchmark...\n");

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
            for _ in 0..SAMPLE_COUNT {
                match run_sample(&command, &re_result, &re_time) {
                    Ok(sample) => {
                        samples.push(sample);
                        print!(".");
                    }
                    Err(error) => {
                        println!(" Failed: {error}");
                        break;
                    }
                }
                std::io::stdout().flush().unwrap();
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
        }
    }

    results.sort_by(|left, right| {
        left.benchmark
            .cmp(&right.benchmark)
            .then(left.script_time_ms.cmp(&right.script_time_ms))
    });
    print_results(results.as_slice());
}

fn compile_standalone(benchmark: &BenchmarkCase) -> bool {
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

fn run_sample(
    command: &[String],
    re_result: &Regex,
    re_time: &Regex,
) -> Result<BenchmarkSample, String> {
    let mut process = Command::new(&command[0]);
    process.args(&command[1..]).stdout(Stdio::piped());

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
    for benchmark in BENCHMARK_CASES {
        let rows = results
            .iter()
            .filter(|result| result.benchmark == benchmark.name)
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

        println!("\n--- {} ---", benchmark.name);
        if let Ok(table) = table.display() {
            println!("{table}");
        }
    }
}
