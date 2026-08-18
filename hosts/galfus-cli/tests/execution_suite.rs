use std::path::{Path, PathBuf};
use std::process::Command;

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
    exit_code: i32,
    stdout: Option<String>,
    diagnostic: Option<String>,
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/execution")
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
        let output = Command::new(executable)
            .arg(&case.command)
            .arg(&target)
            .args(&case.args)
            .output()
            .unwrap_or_else(|error| panic!("{}: CLI must start: {error}", case.name));
        let actual_exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if actual_exit_code != case.exit_code {
            failures.push(format!(
                "{}: expected exit code {}, got {actual_exit_code}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                case.name, case.exit_code,
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
