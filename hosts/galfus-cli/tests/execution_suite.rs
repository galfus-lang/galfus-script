use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Manifest {
    case: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    path: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    exit_code: Option<i32>,
    stdout: Option<String>,
    diagnostic: Option<String>,
    http_request: Option<HttpRequest>,
}

#[derive(Debug, Clone, Deserialize)]
struct HttpRequest {
    address: String,
    path: String,
    expected_status: u16,
    #[serde(default = "default_http_concurrency")]
    concurrency: usize,
    #[serde(default = "default_http_rounds")]
    rounds: usize,
}

const fn default_http_concurrency() -> usize {
    1
}

const fn default_http_rounds() -> usize {
    1
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/execution")
}

fn request_status(request: &HttpRequest) -> Result<u16, String> {
    let address: SocketAddr = request.address.parse().map_err(|error| {
        format!(
            "invalid HTTP fixture address `{}`: {error}",
            request.address
        )
    })?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(250))
        .map_err(|error| format!("could not connect to {}: {error}", request.address))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|error| format!("could not configure HTTP read timeout: {error}"))?;
    stream
        .write_all(
            format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                request.path, request.address
            )
            .as_bytes(),
        )
        .map_err(|error| format!("could not write HTTP request: {error}"))?;

    let mut response = [0; 1024];
    let response_len = stream
        .read(&mut response)
        .map_err(|error| format!("could not read HTTP response: {error}"))?;
    let response = String::from_utf8_lossy(&response[..response_len]);
    let status = response
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| format!("malformed HTTP response: {response:?}"))?
        .parse()
        .map_err(|error| format!("malformed HTTP response status: {error}"))?;
    Ok(status)
}

fn collect_stream(stream: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut stream = stream;
        let mut bytes = Vec::new();
        stream
            .read_to_end(&mut bytes)
            .expect("CLI output stream must be readable");
        bytes
    })
}

fn run_http_case(
    executable: &str,
    target: &Path,
    case: &Case,
    request: &HttpRequest,
) -> Result<(), String> {
    let mut child = Command::new(executable)
        .arg(&case.command)
        .arg(target)
        .args(&case.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{}: CLI must start: {error}", case.name))?;
    let stdout = collect_stream(child.stdout.take().expect("CLI stdout must be piped"));
    let stderr = collect_stream(child.stderr.take().expect("CLI stderr must be piped"));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_error = None;
    let mut server_ready = false;

    while Instant::now() < deadline {
        match request_status(request) {
            Ok(status) => {
                if status != request.expected_status {
                    last_error = Some(format!(
                        "expected HTTP status {}, got {status}",
                        request.expected_status
                    ));
                } else {
                    server_ready = true;
                }
                break;
            }
            Err(error) => last_error = Some(error),
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("{}: could not inspect CLI status: {error}", case.name))?
        {
            last_error = Some(format!("CLI exited with {status}"));
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let concurrent_error = server_ready
        .then(|| {
            (0..request.rounds).find_map(|_| {
                (0..request.concurrency)
                    .map(|_| {
                        let request = request.clone();
                        std::thread::spawn(move || request_status(&request))
                    })
                    .find_map(|result| {
                        match result.join().expect("HTTP request worker must finish") {
                            Ok(status) if status == request.expected_status => None,
                            Ok(status) => Some(format!(
                                "expected HTTP status {}, got {status}",
                                request.expected_status
                            )),
                            Err(error) => Some(error),
                        }
                    })
            })
        })
        .flatten();

    if child
        .try_wait()
        .map_err(|error| format!("{}: could not inspect CLI status: {error}", case.name))?
        .is_none()
    {
        child
            .kill()
            .map_err(|error| format!("{}: could not stop HTTP fixture: {error}", case.name))?;
    }
    let exit_status = child
        .wait()
        .map_err(|error| format!("{}: could not wait for HTTP fixture: {error}", case.name))?;
    let stdout_bytes = stdout.join().expect("stdout reader must finish");
    let stderr_bytes = stderr.join().expect("stderr reader must finish");
    let stdout = String::from_utf8_lossy(&stdout_bytes);
    let stderr = String::from_utf8_lossy(&stderr_bytes);

    match concurrent_error {
        None if server_ready => Ok(()),
        Some(error) => Err(format!(
            "{}: concurrent request failed ({error})\nCLI exit: {exit_status}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            case.name,
        )),
        None => Err(format!(
            "{}: server did not return HTTP status {} ({})\nCLI exit: {exit_status}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            case.name,
            request.expected_status,
            last_error.unwrap_or_else(|| "request timed out".to_string()),
        )),
    }
}

#[test]
fn execution_manifest_matches_cli_behavior() {
    let root = fixture_root();
    let manifest_text = std::fs::read_to_string(root.join("manifest.toml"))
        .expect("execution manifest must be readable");
    let manifest: Manifest =
        toml::from_str(&manifest_text).expect("execution manifest must be valid TOML");
    let executable = env!("CARGO_BIN_EXE_galfus-cli");
    let mut failures = Vec::new();

    for case in manifest.case {
        let target = root.join(&case.path);
        if let Some(request) = &case.http_request {
            if let Err(error) = run_http_case(executable, &target, &case, request) {
                failures.push(error);
            }
            continue;
        }
        let output = Command::new(executable)
            .arg(&case.command)
            .arg(&target)
            .args(&case.args)
            .output()
            .unwrap_or_else(|error| panic!("{}: CLI must start: {error}", case.name));
        let actual_exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let expected_exit_code = case
            .exit_code
            .expect("non-HTTP execution cases must declare an exit code");
        if actual_exit_code != expected_exit_code {
            failures.push(format!(
                "{}: expected exit code {}, got {actual_exit_code}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                case.name, expected_exit_code,
            ));
        }
        if let Some(expected_stdout) = case.stdout
            && stdout != expected_stdout
        {
            failures.push(format!(
                "{}: expected stdout `{expected_stdout:?}`, got `{stdout:?}`",
                case.name,
            ));
        }
        if let Some(expected_diagnostic) = case.diagnostic
            && !stdout.contains(&expected_diagnostic)
            && !stderr.contains(&expected_diagnostic)
        {
            failures.push(format!(
                    "{}: missing diagnostic `{expected_diagnostic}`\nstdout:\n{stdout}\nstderr:\n{stderr}",
                    case.name,
                ));
        }
    }

    assert!(
        failures.is_empty(),
        "execution suite failures:\n{}",
        failures.join("\n\n")
    );
}
