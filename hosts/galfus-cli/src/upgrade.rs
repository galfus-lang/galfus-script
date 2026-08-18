use std::env;
use std::fs;
use std::io::Read;

use anyhow::{Result, anyhow};
use serde::Deserialize;

use crate::ReleaseTag;

const MANIFEST_URL: &str = "https://storage.galfus.com/manifest.json";

#[derive(Debug, Deserialize)]
struct Manifest {
    latest_tag: String,
    tags: std::collections::HashMap<String, String>,
}

pub fn run_upgrade(tag: ReleaseTag) -> Result<()> {
    println!("=> Fetching manifest from {}...", MANIFEST_URL);

    // Fetch manifest
    let manifest_json: Manifest = ureq::get(MANIFEST_URL).call()?.into_json()?;

    let resolved_tag = match tag {
        ReleaseTag::Latest => manifest_json.latest_tag.clone(),
        ReleaseTag::Alpha => "alpha".to_string(),
        ReleaseTag::Beta => "beta".to_string(),
        ReleaseTag::Stable => "stable".to_string(),
        ReleaseTag::Next => "next".to_string(),
    };

    println!("=> Selected tag: {}", resolved_tag);

    let version = manifest_json
        .tags
        .get(&resolved_tag)
        .ok_or_else(|| anyhow!("Tag '{}' not found in the manifest.", resolved_tag))?;

    println!("=> Version: {}", version);

    let os = match env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        "windows" => "windows",
        other => return Err(anyhow!("Unsupported OS: {}", other)),
    };

    let arch = match env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => return Err(anyhow!("Unsupported Architecture: {}", other)),
    };

    let mut download_url = format!(
        "https://storage.galfus.com/cli/{}/{}/{}/{}/galfus-cli-{}-{}",
        resolved_tag, version, os, arch, os, arch
    );

    // Windows executable extension handling
    if os == "windows" {
        download_url.push_str(".exe");
    }

    println!("=> Downloading from {}...", download_url);

    let response = ureq::get(&download_url).call()?;

    let mut reader = response.into_reader();
    let mut binary_data = Vec::new();
    reader.read_to_end(&mut binary_data)?;

    // Basic HTML error page check like in the shell script
    if binary_data.starts_with(b"<") {
        return Err(anyhow!(
            "Failed to download binary (File not found on CDN or returned HTML)."
        ));
    }

    let temp_dir = env::temp_dir();
    let temp_file_name = if os == "windows" {
        "galfus_update_tmp.exe"
    } else {
        "galfus_update_tmp"
    };
    let temp_path = temp_dir.join(temp_file_name);

    fs::write(&temp_path, &binary_data)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&temp_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&temp_path, perms)?;
    }

    println!("=> Replacing current executable...");
    self_replace::self_replace(&temp_path)?;

    // Clean up temp file
    let _ = fs::remove_file(&temp_path);

    println!("=> Galfus upgrade complete!");
    Ok(())
}
