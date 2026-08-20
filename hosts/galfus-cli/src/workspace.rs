#[cfg(test)]
mod tests;

use std::fs;
use std::sync;

use anyhow::{Context, Result, bail};
use galfus_workspace::{LoadResult, Workspace};
use std::path::Path;

pub fn check_workspace_root(root: &str) -> Result<()> {
    let mut workspace = load_workspace(Path::new(root))?;
    let (is_valid, diagnostics) = {
        let report = workspace.check();
        (report.is_valid, report.diagnostics.clone())
    };
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
}

pub fn run_project(root: &str, cli_args: &[String]) -> Result<i32> {
    let mut workspace = load_workspace(Path::new(root))?;
    let (is_valid, diagnostics) = {
        let report = workspace.check();
        (report.is_valid, report.diagnostics.clone())
    };
    if !is_valid {
        crate::diagnostics::print_diagnostics(&diagnostics, &workspace.source_state.store);
        bail!("workspace validation failed");
    }
    let compile_report = workspace
        .compile()
        .and_then(|_| workspace.optimize())
        .map_err(|error| anyhow::anyhow!("workspace compilation failed: {error:?}"))?;
    let args = cli_args
        .iter()
        .map(|argument| argument.as_bytes().to_vec())
        .collect::<Vec<_>>();

    if let Ok(_) = std::env::var("GALFUS_DEBUG_BYTECODE") {
        println!("{:#?}", compile_report.package.graph());
    }

    let providers =
        galfus_host_native::providers::default_providers(compile_report.package.metadata().clone());
    let driver = std::rc::Rc::new(galfus_host_native::driver::NativeDriver::new());

    let host = galfus_host_native::ExecutionHost::new(
        providers,
        galfus_contract::AdapterBindings::default(),
        driver,
    );

    let code = match host.run(compile_report.package.clone(), args.as_slice()) {
        Ok(code) => code,
        Err(failure) => {
            let style = dialoguer::console::style;
            eprintln!(
                "{}: {}",
                style("Runtime Error").red().bold(),
                failure.message
            );
            if !failure.stack.is_empty() {
                eprintln!("\n{}", style("Stack trace:").yellow().bold());
                for frame in &failure.stack {
                    let module_id = galfus_core::ModuleId::new(frame.module_id as u32);
                    let module_name = match compile_report.package.graph().get(module_id) {
                        Some(m) => m.path().as_str().to_string(),
                        None => format!("<module {}>", frame.module_id),
                    };
                    eprintln!(
                        "  at \x1b[36m{}\x1b[0m offset {}",
                        module_name, frame.instruction_offset
                    );
                }
            }
            bail!("execution failed");
        }
    };
    Ok(code)
}

pub fn load_workspace(root: &Path) -> Result<Workspace> {
    if root.is_file() {
        return load_source_file(root);
    }

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
