use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use galfus_contract::CURRENT_PRODUCER_VERSION;
use galfus_workspace::config::ModuleTarget;

use crate::workspace::load_workspace;

pub const STORAGE_URL: &str = "https://storage.galfus.com";

pub fn run_compile(
    root: &str,
    target: Option<String>,
    out: Option<String>,
    profile: &str,
) -> Result<()> {
    let workspace_path = Path::new(root);
    let mut workspace = load_workspace(workspace_path)?;
    let report = workspace.check();
    if !report.is_valid {
        bail!("workspace validation failed: {:?}", report.diagnostics);
    }

    if workspace.config.as_ref().unwrap().target() == ModuleTarget::Lib {
        bail!("Cannot compile a library project. Use 'galfus check' instead.");
    }

    let compile_report = workspace
        .compile()
        .map_err(|error| anyhow::anyhow!("workspace compilation failed: {error:?}"))?;

    let bytecode = compile_report
        .package
        .to_bytecode()
        .map_err(|error| anyhow::anyhow!("failed to encode bytecode: {:?}", error))?;

    let resolved_target = target.unwrap_or_else(get_default_target);
    let major = CURRENT_PRODUCER_VERSION.major;
    let minor = CURRENT_PRODUCER_VERSION.minor;
    let patch = CURRENT_PRODUCER_VERSION.patch;
    let version_str = format!("{}.{}.{}", major, minor, patch);
    let tag = CURRENT_PRODUCER_VERSION.tag().unwrap_or("stable");

    let cache_dir = dirs::home_dir()
        .context("Could not determine home directory")?
        .join(".galfus")
        .join("hosts")
        .join(tag)
        .join(&version_str);

    fs::create_dir_all(&cache_dir).context("failed to create cache directory")?;

    let project_name = workspace.config.as_ref().unwrap().name().to_string();

    if resolved_target == "web" {
        compile_web(
            &cache_dir,
            &version_str,
            tag,
            profile,
            out,
            bytecode,
            &project_name,
        )
    } else {
        compile_native(
            &cache_dir,
            &version_str,
            tag,
            profile,
            &resolved_target,
            out,
            bytecode,
            &project_name,
        )
    }
}

fn compile_web(
    cache_dir: &Path,
    version: &str,
    tag: &str,
    profile: &str,
    out: Option<String>,
    bytecode: Vec<u8>,
    project_name: &str,
) -> Result<()> {
    let out_dir = out
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dist"));
    fs::create_dir_all(&out_dir).context("failed to create output directory")?;

    let host_name = format!("galfus-host-web-{}", profile);
    let target_cache_dir = cache_dir.join(&host_name);
    fs::create_dir_all(&target_cache_dir).context("failed to create target cache directory")?;

    // Base URL: {STORAGE_URL}/host-web/<tag>/<version>/web/wasm32/<host_name>/
    let base_url = format!(
        "{}/host-web/{}/{}/web/wasm32/{}",
        STORAGE_URL, tag, version, host_name
    );

    let files = [
        "galfus_host_web_bg.wasm",
        "galfus_host_web_bg.wasm.d.ts",
        "galfus_host_web.d.ts",
        "galfus_host_web.js",
    ];

    for file_name in files {
        let file_path = target_cache_dir.join(file_name);
        if !file_path.exists() {
            let url = format!("{}/{}", base_url, file_name);
            download_file(&url, &file_path)?;
        }
        // Copy to dist/
        let dest = out_dir.join(file_name);
        fs::copy(&file_path, &dest)
            .with_context(|| format!("failed to copy {} to {:?}", file_name, dest))?;
    }

    // Write app.bin
    let bin_path = out_dir.join(format!("{}.bin", project_name));
    fs::write(&bin_path, bytecode).context("failed to write bytecode")?;

    // Write boilerplate
    let index_js = format!(
        r#"import init, {{ start }} from './galfus_host_web.js';

async function main() {{
    await init();
    const response = await fetch('./{}.bin');
    const buffer = await response.arrayBuffer();
    const exitCode = await start({{ blob: new Uint8Array(buffer) }});
    console.log("Process exited with code:", exitCode);
}}

main().catch(console.error);
"#,
        project_name
    );
    fs::write(out_dir.join("index.js"), index_js)?;

    let index_html = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Galfus Web App</title>
</head>
<body>
    <script type="module" src="./index.js"></script>
</body>
</html>
"#;
    fs::write(out_dir.join("index.html"), index_html)?;

    println!("Successfully compiled web bundle to {:?}", out_dir);
    Ok(())
}

fn compile_native(
    cache_dir: &Path,
    version: &str,
    tag: &str,
    profile: &str,
    target: &str,
    out: Option<String>,
    bytecode: Vec<u8>,
    project_name: &str,
) -> Result<()> {
    let host_name = format!("galfus-{}-x64-{}", target, profile);
    let mut file_name = host_name.clone();
    if target == "windows" {
        file_name.push_str(".exe");
    }

    let host_path = cache_dir.join(&file_name);

    if !host_path.exists() {
        // Base URL: {STORAGE_URL}/host-native/<tag>/<version>/<target>/x64/<file_name>
        let url = format!(
            "{}/host-native/{}/{}/{}/x64/{}",
            STORAGE_URL, tag, version, target, file_name
        );
        download_file(&url, &host_path)?;
    }

    let host_bytes = fs::read(&host_path)
        .with_context(|| format!("failed to read host binary at {:?}", host_path))?;

    let out_path = out.map(PathBuf::from).unwrap_or_else(|| {
        let mut p = PathBuf::from("dist").join(project_name);
        if target == "windows" {
            p.set_extension("exe");
        }
        p
    });

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).context("failed to create output directory")?;
    }

    let mut out_file = fs::File::create(&out_path)
        .with_context(|| format!("failed to create output file at {:?}", out_path))?;

    use std::io::Write;
    out_file.write_all(&host_bytes)?;
    out_file.write_all(&bytecode)?;

    let payload_size = bytecode.len() as u64;
    out_file.write_all(&payload_size.to_le_bytes())?;

    const MAGIC_MARKER: &[u8; 8] = b"GLFS_PKG";
    out_file.write_all(MAGIC_MARKER)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = out_file.metadata()?.permissions();
        perms.set_mode(0o755);
        out_file.set_permissions(perms)?;
    }

    println!(
        "Successfully compiled standalone executable to {:?}",
        out_path
    );
    Ok(())
}

fn download_file(url: &str, dest: &Path) -> Result<()> {
    println!("Downloading {} ...", url);
    let response = ureq::get(url)
        .call()
        .with_context(|| format!("Failed to GET {}", url))?;

    if response.status() != 200 {
        bail!("Failed to download {}: HTTP {}", url, response.status());
    }

    let mut reader = response.into_reader();
    let mut out_file =
        fs::File::create(dest).with_context(|| format!("failed to create file {:?}", dest))?;
    std::io::copy(&mut reader, &mut out_file)?;
    Ok(())
}

fn get_default_target() -> String {
    if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if cfg!(target_os = "macos") {
        "macos".to_string()
    } else {
        "linux".to_string()
    }
}
