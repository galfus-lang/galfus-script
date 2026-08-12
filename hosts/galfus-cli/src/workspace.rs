#[cfg(test)]
mod tests;

use std::fs;
use std::sync;

use anyhow::{Context, Result, bail};
use galfus_workspace::{LoadResult, Workspace};
use std::path::Path;

pub fn check_workspace_root(root: &str) -> Result<()> {
    let mut workspace = load_workspace(Path::new(root))?;
    let report = workspace.check();
    for diagnostic in report.diagnostics.iter() {
        println!(
            "{:?} {}: {}",
            diagnostic.severity(),
            diagnostic.code().as_str(),
            diagnostic.message()
        );
    }
    if report.is_valid {
        Ok(())
    } else {
        bail!("workspace validation failed")
    }
}

pub fn compile_workspace(root: &str, target: &str, out: &str, profile: &str) -> Result<()> {
    let mut workspace = load_workspace(Path::new(root))?;
    let report = workspace.check();
    if !report.is_valid {
        bail!("workspace validation failed: {:?}", report.diagnostics);
    }

    let compile_report = workspace
        .compile()
        .map_err(|error| anyhow::anyhow!("workspace compilation failed: {error:?}"))?;

    let bytecode = compile_report
        .package
        .to_bytecode()
        .map_err(|error| anyhow::anyhow!("failed to encode bytecode: {:?}", error))?;

    let mut host_name = format!("galfus-{}-{}", target, profile);
    if target.contains("windows") {
        host_name.push_str(".exe");
    }

    let host_path = Path::new("build").join(host_name);
    if !host_path.exists() {
        bail!(
            "Host executable not found at {:?}. Please build it first using `bun cmd hosts build --target {} -p {}`",
            host_path,
            target,
            profile
        );
    }

    let host_bytes = fs::read(&host_path)
        .with_context(|| format!("failed to read host binary at {:?}", host_path))?;

    let mut out_file = fs::File::create(out)
        .with_context(|| format!("failed to create output file at {}", out))?;

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

    println!("Successfully compiled standalone executable to {}", out);

    Ok(())
}

pub fn run_project(root: &str, cli_args: &[String]) -> Result<i32> {
    let mut workspace = load_workspace(Path::new(root))?;
    let report = workspace.check();
    if !report.is_valid {
        bail!("workspace validation failed: {:?}", report.diagnostics);
    }
    let compile_report = workspace
        .compile()
        .map_err(|error| anyhow::anyhow!("workspace compilation failed: {error:?}"))?;
    let args = cli_args
        .iter()
        .map(|argument| argument.as_bytes().to_vec())
        .collect::<Vec<_>>();

    let providers =
        galfus_host_native::providers::default_providers(compile_report.package.metadata().clone());
    let driver = std::rc::Rc::new(galfus_host_native::driver::NativeDriver::new());

    let host = galfus_host_native::ExecutionHost::new(
        providers,
        galfus_contract::AdapterBindings::default(),
        driver,
    );

    let code = host.run(compile_report.package.clone(), args.as_slice())?;
    Ok(code)
}

fn load_workspace(root: &Path) -> Result<Workspace> {
    if root.is_file() {
        return load_source_file(root);
    }

    let root = root
        .canonicalize()
        .context("workspace root does not exist")?;
    let config = fs::read(root.join("galfus.toml"))?;

    let mut workspace = workspace_with_native_catalog();
    if let LoadResult::Diagnostics(diagnostics) = workspace
        .load_config(config.as_slice())
        .map_err(|error| anyhow::anyhow!("workspace configuration error: {error:?}"))?
    {
        bail!("workspace configuration failed: {diagnostics:?}");
    }

    load_sources(&mut workspace, root.as_path(), root.as_path())?;
    Ok(workspace)
}

fn load_source_file(file: &Path) -> Result<Workspace> {
    if file.extension().is_none_or(|extension| extension != "gfs") {
        bail!("source file must use the .gfs extension");
    }

    let file = file.canonicalize().context("source file does not exist")?;
    let module_path = file
        .file_name()
        .and_then(|name| name.to_str())
        .context("source file name is not valid UTF-8")?;
    let source = fs::read(file.as_path())?;

    let mut workspace = workspace_with_native_catalog();
    let config = format!(
        "[module]\nname = \"single-file\"\ntarget = \"app\"\n[entry]\npath = \"{module_path}\"\n"
    );
    if let LoadResult::Diagnostics(diagnostics) = workspace
        .load_config(config.as_bytes())
        .map_err(|error| anyhow::anyhow!("workspace configuration error: {error:?}"))?
    {
        bail!("workspace configuration failed: {diagnostics:?}");
    }

    workspace
        .load_module(module_path, source.as_slice())
        .map_err(|error| anyhow::anyhow!("workspace source error: {error:?}"))?;
    Ok(workspace)
}

fn workspace_with_native_catalog() -> Workspace {
    let mut workspace = Workspace::new();
    workspace.set_catalog(sync::Arc::new(galfus_host_native::native_catalog()));
    workspace
}

fn load_sources(workspace: &mut Workspace, workspace_root: &Path, directory: &Path) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            load_sources(workspace, workspace_root, path.as_path())?;
            continue;
        }
        if !file_type.is_file() || path.extension().is_none_or(|extension| extension != "gfs") {
            continue;
        }

        let source = fs::read(path.as_path())?;
        let module_path = path
            .strip_prefix(workspace_root)
            .context("source module is outside the workspace root")?;
        let module_path = module_path.to_string_lossy().replace('\\', "/");
        workspace
            .load_module(module_path.as_str(), source.as_slice())
            .map_err(|error| anyhow::anyhow!("workspace source error: {error:?}"))?;
    }
    Ok(())
}
