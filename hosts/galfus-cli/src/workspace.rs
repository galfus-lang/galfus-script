#[cfg(test)]
mod tests;

use std::fs;
use std::sync;

use crate::native_io::NativeIoProvider;
use anyhow::{Context, Result, bail};
use galfus_contract::Providers;
use galfus_runtime::CooperativeDriver;
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
    use galfus_contract::KernelDriver;
    let executor = std::rc::Rc::new(CooperativeDriver::new());
    let exit_code = sync::Arc::new(sync::Mutex::new(0));
    let ec = sync::Arc::clone(&exit_code);
    executor.on_exit(Box::new(move |res| {
        *ec.lock().unwrap() = res.unwrap();
    }));

    let preflight = galfus_workspace::ExternalBindingPreflight::new();
    // Note: CLI doesn't currently register external loaders, but it should run the preflight
    // to catch any unmet requirements.
    let mut properties = std::collections::BTreeMap::new();
    properties.insert("os".to_string(), std::env::consts::OS.to_string());
    properties.insert("arch".to_string(), std::env::consts::ARCH.to_string());
    properties.insert("family".to_string(), std::env::consts::FAMILY.to_string());
    // Construct a fallback target triple since env!("TARGET") isn't available
    let target_env = if cfg!(target_env = "msvc") {
        "msvc"
    } else if cfg!(target_env = "musl") {
        "musl"
    } else {
        "gnu"
    };
    let target_triple = format!(
        "{}-unknown-{}-{}",
        std::env::consts::ARCH,
        std::env::consts::OS,
        target_env
    );
    properties.insert("target_triple".to_string(), target_triple);

    let context = galfus_contract::ExternalLoadContext { properties };

    let bindings = preflight
        .run(&compile_report.external_requirements, &context)
        .map_err(|error| anyhow::anyhow!("package preflight failed: {error:?}"))?;

    workspace
        .run_with_bindings(
            args.as_slice(),
            Some(Providers::with_host(Box::new(NativeIoProvider))),
            bindings,
            executor.clone(),
        )
        .map_err(|error| anyhow::anyhow!("workspace execution failed: {error:?}"))?;

    let code = *exit_code.lock().unwrap();
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

    let mut workspace = workspace_with_native_io_catalog();
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

    let mut workspace = workspace_with_native_io_catalog();
    let config =
        format!("[module]\nname = \"single-file\"\ntarget = \"app\"\nentry = \"{module_path}\"\n");
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

fn workspace_with_native_io_catalog() -> Workspace {
    let mut workspace = Workspace::new();
    let catalog = galfus_contract::CapabilityCatalog::new(
        vec![galfus_contract::BridgeModule::new(
            "std/io",
            galfus_contract::STD_IO_SOURCE,
        )],
        Vec::new(),
    )
    .expect("the built-in std/io provider catalog is valid");
    workspace.set_catalog(sync::Arc::new(catalog));
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
