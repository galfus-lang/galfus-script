#[cfg(feature = "wasm")]
mod wasm;

use anyhow::Result;
use galfus_workspace::{LoadResult, Workspace};

/// Stateful facade for embedding a Galfus workspace in a playground host.
pub struct Playground {
    workspace: Workspace,
}

pub struct PlaygroundCheckResult {
    pub is_valid: bool,
    pub diagnostics: Vec<galfus_core::Diagnostic>,
}

impl Default for Playground {
    fn default() -> Self {
        Self::new()
    }
}

impl Playground {
    pub fn new() -> Self {
        let mut workspace = Workspace::new();
        let catalog = std::sync::Arc::new(
            galfus_contract::CapabilityCatalog::new(
                vec![galfus_contract::BridgeModule::new(
                    "std/io",
                    galfus_contract::builtins::STD_IO_SOURCE,
                )],
                Vec::new(),
            )
            .expect("the built-in std/io provider catalog is valid"),
        );
        workspace.set_catalog(catalog);
        let manifest = galfus_workspace::WorkspaceManifest {
            module: Some(galfus_workspace::config::ModuleManifest {
                name: Some("playground".to_string()),
                target: Some("app".to_string()),
                ..Default::default()
            }),
            entry: Some(galfus_workspace::config::EntryManifest {
                path: Some("src/main.gfs".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        workspace
            .load_manifest(manifest)
            .expect("the built-in playground configuration is valid");
        Self { workspace }
    }

    pub fn set_config(&mut self, config_json: &[u8]) -> Result<()> {
        let manifest: galfus_workspace::WorkspaceManifest = serde_json::from_slice(config_json)
            .map_err(|error| anyhow::anyhow!("invalid playground json configuration: {error:?}"))?;

        match self
            .workspace
            .load_manifest(manifest)
            .map_err(|error| anyhow::anyhow!("playground configuration error: {error:?}"))?
        {
            LoadResult::Success => Ok(()),
            LoadResult::Diagnostics(diagnostics) => Err(anyhow::anyhow!(
                "playground configuration diagnostics: {diagnostics:?}"
            )),
        }
    }

    pub fn set_source(&mut self, path: &str, source: &[u8]) -> Result<()> {
        match self
            .workspace
            .load_module(path, source)
            .map_err(|error| anyhow::anyhow!("playground source error: {error:?}"))?
        {
            LoadResult::Success => Ok(()),
            LoadResult::Diagnostics(diagnostics) => Err(anyhow::anyhow!(
                "playground source diagnostics: {diagnostics:?}"
            )),
        }
    }

    pub fn check(&mut self) -> PlaygroundCheckResult {
        let check = self.workspace.check();
        PlaygroundCheckResult {
            is_valid: check.is_valid,
            diagnostics: check.diagnostics.iter().cloned().collect(),
        }
    }

    pub fn compile(&mut self) -> Result<()> {
        self.workspace
            .compile()
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!("playground compilation failed: {error:?}"))
    }

    // workspace é exposto para que wasm.rs possa invocar a execução.
    pub fn get_workspace(&mut self) -> &mut Workspace {
        &mut self.workspace
    }
}
