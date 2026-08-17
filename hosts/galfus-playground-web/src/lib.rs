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
    pub diagnostics: String,
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
        workspace
            .load_config(PLAYGROUND_CONFIG.as_bytes())
            .expect("the built-in playground configuration is valid");
        Self { workspace }
    }

    pub fn set_config(&mut self, config: &[u8]) -> Result<()> {
        match self
            .workspace
            .load_config(config)
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
            diagnostics: format!("{:?}", check.diagnostics),
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

pub const PLAYGROUND_CONFIG: &str =
    "[module]\nname = \"playground\"\ntarget = \"app\"\n[entry]\npath = \"src/main.gfs\"\n";
