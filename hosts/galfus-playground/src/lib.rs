mod buffer_io;
mod executor;

#[cfg(feature = "wasm")]
mod wasm;

#[cfg(test)]
mod tests;

use anyhow::Result;
use galfus_contract::Providers;
use galfus_runtime::{CooperativeDriver, Execution};
use galfus_workspace::{LoadResult, Workspace};

pub use buffer_io::BufferIoProvider;

/// Stateful facade for embedding a Galfus workspace in a playground host.
pub struct Playground {
    workspace: Workspace,
    io: BufferIoProvider,
    execution: Option<Execution>,
}

pub struct PlaygroundCheckResult {
    pub is_valid: bool,
    pub diagnostics: String,
}

pub struct PlaygroundResult {
    pub output: String,
    pub exit_code: i32,
    pub error: Option<String>,
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
        Self {
            workspace,
            io: BufferIoProvider::default(),
            execution: None,
        }
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

    pub fn send_read_data(&self, bytes: &[u8]) {
        self.io.send_read_data(bytes);
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

    pub fn run(&mut self, args: &[Vec<u8>]) -> Result<i32> {
        let executor = std::rc::Rc::new(CooperativeDriver::new());
        let mut execution = self
            .workspace
            .start_execution(
                args,
                Some(Providers::new().with_host("io", Box::new(self.io.clone()))),
                executor.clone(),
            )
            .map_err(|error| anyhow::anyhow!("playground execution failed: {error:?}"))?;
        match execution.run_sync_to_completion()? {
            galfus_contract::BoundaryValue::I32(code) => Ok(code),
            _ => Ok(0),
        }
    }

    pub fn start(&mut self, args: &[Vec<u8>]) -> Result<()> {
        let executor = std::rc::Rc::new(executor::PlaygroundExecutor::new());
        let execution = self
            .workspace
            .start_execution(
                args,
                Some(Providers::new().with_host("io", Box::new(self.io.clone()))),
                executor.clone(),
            )
            .map_err(|error| anyhow::anyhow!("playground execution failed: {error:?}"))?;

        self.execution = Some(execution);
        Ok(())
    }

    pub fn step(&mut self) -> Result<galfus_contract::ExecutorStepResult> {
        if let Some(execution) = &mut self.execution {
            execution
                .poll(100)
                .map_err(|error| anyhow::anyhow!("playground step failed: {error}"))
        } else {
            Err(anyhow::anyhow!("playground execution has not been started"))
        }
    }

    pub fn take_output(&self) -> Vec<u8> {
        self.io.take_output()
    }

    #[cfg(feature = "wasm")]
    pub fn set_write_callback(&self, callback: js_sys::Function) {
        self.io.set_write_callback(callback);
    }
}

pub fn run_source(code: &str, args: &[&str]) -> PlaygroundResult {
    match run_source_inner(code, args) {
        Ok(result) => result,
        Err(error) => PlaygroundResult {
            output: String::new(),
            exit_code: 1,
            error: Some(error.to_string()),
        },
    }
}

fn run_source_inner(code: &str, args: &[&str]) -> Result<PlaygroundResult> {
    let mut playground = Playground::new();
    playground.set_config(PLAYGROUND_CONFIG.as_bytes())?;
    playground.set_source("src/main.gfs", code.as_bytes())?;

    let check = playground.check();
    if !check.is_valid {
        return Ok(PlaygroundResult {
            output: String::new(),
            exit_code: 1,
            error: Some(check.diagnostics),
        });
    }

    let args_bytes = args
        .iter()
        .map(|arg| arg.as_bytes().to_vec())
        .collect::<Vec<_>>();
    playground.compile()?;
    let exit_code = playground.run(args_bytes.as_slice())?;

    let output = String::from_utf8_lossy(playground.take_output().as_slice()).into_owned();

    Ok(PlaygroundResult {
        output,
        exit_code,
        error: None,
    })
}

pub const PLAYGROUND_CONFIG: &str =
    "[module]\nname = \"playground\"\ntarget = \"app\"\n[entry]\npath = \"src/main.gfs\"\n";
