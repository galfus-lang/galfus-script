mod filesystem_loader;
#[cfg(test)]
mod tests;
mod timing;

use std::fs;
use std::sync;

use anyhow::{Context, Result, bail};
use galfus_workspace::{LoadResult, Workspace};
use std::path::Path;
use std::time::{Duration, Instant};

use filesystem_loader::FilesystemSourceLoader;
use timing::WorkspaceTimer;

pub fn check_workspace_root_from_cli(root: &str, process_bootstrap: Duration) -> Result<()> {
    let mut timer = WorkspaceTimer::new(process_bootstrap);
    let result = (|| {
        let mut workspace = load_workspace_with_timer(Path::new(root), &mut timer)?;
        workspace.set_timing_collector(timer.collector());
        let started = Instant::now();
        let (is_valid, diagnostics) = {
            let report = workspace.check();
            (report.is_valid, report.diagnostics.clone())
        };
        timer.add_interface_validation(started.elapsed());
        crate::diagnostics::print_diagnostics(&diagnostics, &workspace.source_state.store);
        if is_valid {
            println!(
                "{}",
                dialoguer::console::style("✔ Workspace is valid!")
                    .green()
                    .bold()
            );
            Ok(())
        } else {
            bail!("workspace validation failed")
        }
    })();
    timer.write();
    result
}

#[cfg(test)]
pub fn run_project(root: &str, cli_args: &[String]) -> Result<i32> {
    run_project_from_cli(root, cli_args, Duration::ZERO)
}

pub fn run_project_from_cli(
    root: &str,
    cli_args: &[String],
    process_bootstrap: Duration,
) -> Result<i32> {
    let mut timer = WorkspaceTimer::new(process_bootstrap);
    let result = (|| {
        let mut workspace = load_workspace_with_timer(Path::new(root), &mut timer)?;
        workspace.set_timing_collector(timer.collector());
        let started = Instant::now();
        let (is_valid, diagnostics) = {
            let report = workspace.check();
            (report.is_valid, report.diagnostics.clone())
        };
        timer.add_interface_validation(started.elapsed());
        if !is_valid {
            crate::diagnostics::print_diagnostics(&diagnostics, &workspace.source_state.store);
            bail!("workspace validation failed");
        }
        let args = cli_args
            .iter()
            .map(|argument| argument.as_bytes().to_vec())
            .collect::<Vec<_>>();
        let providers =
            galfus_host_native::providers::default_providers(workspace.package_metadata());
        let driver = std::rc::Rc::new(galfus_host_native::driver::NativeDriver::new());
        let mut execution = workspace
            .start_execution(args.as_slice(), Some(providers), driver)
            .map_err(|error| anyhow::anyhow!("execution failed: {error:?}"))?;
        let started = Instant::now();
        let result = execution
            .run_sync_to_completion()
            .map_err(|error| anyhow::anyhow!("execution failed: {error:?}"));
        timer.set_entry_completion(started.elapsed());
        result
    })();
    timer.write();
    result
}

pub fn load_workspace(root: &Path) -> Result<Workspace> {
    let mut timer = WorkspaceTimer::new(Duration::ZERO);
    load_workspace_with_timer(root, &mut timer)
}

fn load_workspace_with_timer(root: &Path, timer: &mut WorkspaceTimer) -> Result<Workspace> {
    if root.is_file() {
        return load_source_file(root, timer);
    }

    let started = Instant::now();
    let root = root
        .canonicalize()
        .context("workspace root does not exist")?;
    let config_string = fs::read_to_string(root.join("galfus.toml"))?;
    let manifest = toml::from_str::<galfus_workspace::WorkspaceManifest>(&config_string)
        .context("invalid galfus.toml format")?;

    let mut workspace = workspace_with_native_catalog();
    if let LoadResult::Diagnostics(diagnostics) = workspace
        .load_manifest(manifest)
        .map_err(|error| anyhow::anyhow!("workspace configuration error: {error:?}"))?
    {
        bail!("workspace configuration failed: {diagnostics:?}");
    }
    workspace.set_source_loader(sync::Arc::new(
        FilesystemSourceLoader::new(root.as_path())
            .context("could not create filesystem source loader")?,
    ));
    timer.add_manifest_catalog_setup(started.elapsed());

    load_sources(&mut workspace, root.as_path(), root.as_path(), timer)?;
    Ok(workspace)
}

fn load_source_file(file: &Path, timer: &mut WorkspaceTimer) -> Result<Workspace> {
    if file.extension().is_none_or(|extension| extension != "gfs") {
        bail!("source file must use the .gfs extension");
    }

    let discovery_started = Instant::now();
    let file = file.canonicalize().context("source file does not exist")?;
    let module_path = file
        .file_name()
        .and_then(|name| name.to_str())
        .context("source file name is not valid UTF-8")?;
    timer.add_source_discovery(discovery_started.elapsed());
    let read_started = Instant::now();
    let source = fs::read(file.as_path())?;
    timer.add_source_reads(read_started.elapsed());

    let started = Instant::now();
    let mut workspace = workspace_with_native_catalog();

    let manifest = galfus_workspace::WorkspaceManifest {
        module: Some(galfus_workspace::ModuleManifest {
            name: Some("single-file".to_string()),
            target: Some("app".to_string()),
            ..Default::default()
        }),
        entry: Some(galfus_workspace::EntryManifest {
            path: Some(module_path.to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };

    if let LoadResult::Diagnostics(diagnostics) = workspace
        .load_manifest(manifest)
        .map_err(|error| anyhow::anyhow!("workspace configuration error: {error:?}"))?
    {
        bail!("workspace configuration failed: {diagnostics:?}");
    }
    timer.add_manifest_catalog_setup(started.elapsed());

    workspace
        .load_module(module_path, source.as_slice())
        .map_err(|error| anyhow::anyhow!("workspace source error: {error:?}"))?;
    Ok(workspace)
}

pub fn workspace_with_native_catalog() -> Workspace {
    let mut workspace = Workspace::new();
    workspace.set_catalog(sync::Arc::new(galfus_host_native::native_catalog()));
    workspace
}

fn load_sources(
    workspace: &mut Workspace,
    workspace_root: &Path,
    directory: &Path,
    timer: &mut WorkspaceTimer,
) -> Result<()> {
    let discovery_started = Instant::now();
    let entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    timer.add_source_discovery(discovery_started.elapsed());
    for entry in entries {
        let path = entry.path();
        let discovery_started = Instant::now();
        let file_type = entry.file_type()?;
        timer.add_source_discovery(discovery_started.elapsed());
        if file_type.is_dir() {
            load_sources(workspace, workspace_root, path.as_path(), timer)?;
            continue;
        }
        if !file_type.is_file() || path.extension().is_none_or(|extension| extension != "gfs") {
            continue;
        }

        let read_started = Instant::now();
        let source = fs::read(path.as_path())?;
        timer.add_source_reads(read_started.elapsed());
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
