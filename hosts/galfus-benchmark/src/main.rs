use cli_table::{Cell, Style, Table, format::Justify};
use regex::Regex;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use sysinfo::{Pid, System};

#[derive(Debug)]
struct BenchmarkResult {
    language: String,
    time_ms: u64,
    result: i64,
    peak_mem_mb: f64,
}

fn main() {
    println!("Compiling Galfus Engine...");
    let status = Command::new("cargo")
        .args(["build", "--profile", "release"])
        .status()
        .expect("Failed to compile Galfus");
    if !status.success() {
        eprintln!("Compilation failed!");
        return;
    }

    println!("Compiling standalone Galfus (Host)...");
    let status = Command::new("./target/release/galfus-cli")
        .args([
            "compile",
            "--local-host",
            "./target/release/galfus-host-native",
            "-t",
            "native",
            "-o",
            "./target/release/fib_standalone",
            "benchmark/fib.gfs",
        ])
        .status()
        .expect("Failed to compile standalone host");
    if !status.success() {
        eprintln!("Standalone compilation failed!");
        return;
    }

    let re_result = Regex::new(r"RESULT=(\d+)").unwrap();
    let re_time = Regex::new(r"TIME_MS=(\d+)").unwrap();

    let bun_bin = std::env::var("BUN_INSTALL")
        .map(|v| format!("{}/bin/bun", v))
        .unwrap_or_else(|_| {
            if let Ok(home) = std::env::var("HOME") {
                let path = format!("{}/.bun/bin/bun", home);
                if std::path::Path::new(&path).exists() {
                    return path;
                }
            }
            "bun".to_string()
        });

    let targets = vec![
        (
            "JavaScript (Bun)",
            vec![bun_bin.as_str(), "benchmark/fib.js"],
        ),
        ("Lua JIT", vec!["luajit", "benchmark/fib.lua"]),
        ("Lua 5.4", vec!["lua", "benchmark/fib.lua"]),
        ("Python 3", vec!["python3", "benchmark/fib.py"]),
        (
            "Galfus (Workspace)",
            vec!["./target/release/galfus-cli", "run", "benchmark/fib.gfs"],
        ),
        ("Galfus (Host)", vec!["./target/release/fib_standalone"]),
    ];

    let mut results = vec![];
    println!("\nRunning benchmarks (Fibonacci 35)...\n");

    for (name, cmd_args) in targets {
        print!("Testing {}... ", name);
        use std::io::Write;
        std::io::stdout().flush().unwrap();

        let mut cmd = Command::new(cmd_args[0]);
        cmd.args(&cmd_args[1..]);
        cmd.stdout(Stdio::piped());

        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(_) => {
                println!("Failed or Skipped");
                continue;
            }
        };

        let pid = Pid::from_u32(child.id());

        // Spawn a thread to aggressively poll memory usage
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = thread::spawn(move || {
            let mut local_sys = System::new();
            let mut peak_bytes = 0;
            loop {
                local_sys.refresh_processes();
                if let Some(process) = local_sys.process(pid) {
                    let mem = process.memory(); // memory in bytes
                    if mem > peak_bytes {
                        peak_bytes = mem;
                    }
                }
                if rx.try_recv().is_ok() {
                    break;
                }
                thread::sleep(Duration::from_millis(1));
            }
            peak_bytes
        });

        let output = child.wait_with_output().expect("Failed to wait on child");
        let _ = tx.send(()); // Stop polling thread
        let peak_bytes = handle.join().unwrap();
        let peak_mb = peak_bytes as f64 / 1024.0 / 1024.0;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut res_val = 0;
        let mut time_val = 0;

        if let Some(cap) = re_result.captures(&output_str) {
            res_val = cap[1].parse().unwrap_or(0);
        }
        if let Some(cap) = re_time.captures(&output_str) {
            time_val = cap[1].parse().unwrap_or(0);
        }

        if time_val > 0 && res_val > 0 {
            println!("{}ms", time_val);
            results.push(BenchmarkResult {
                language: name.to_string(),
                time_ms: time_val,
                result: res_val,
                peak_mem_mb: peak_mb,
            });
        } else {
            println!("Failed or Skipped");
        }
    }

    results.sort_by_key(|r| r.time_ms);

    let mut table_rows = vec![];
    for (i, r) in results.iter().enumerate() {
        table_rows.push(vec![
            i.cell(),
            r.language.clone().cell(),
            r.time_ms.cell().justify(Justify::Right),
            format!("{:.2}", r.peak_mem_mb)
                .cell()
                .justify(Justify::Right),
            r.result.cell().justify(Justify::Right),
        ]);
    }

    let table = table_rows.table().title(vec![
        "".cell().bold(true),
        "Language".cell().bold(true),
        "Time (ms)".cell().bold(true),
        "Peak Mem (MB)".cell().bold(true),
        "Result".cell().bold(true),
    ]);

    println!("\n--- Benchmark Results ---");
    if let Ok(print_table) = table.display() {
        println!("{}", print_table);
    }
}
