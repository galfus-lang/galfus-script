//! Host integration contracts for Galfus execution.
//!
//! See the Runtime Ownership Matrix in the Architecture Reference (`docs/Galfus_Architecture_Reference.md`)
//! for authoritative details on the lifecycle and ownership of boundary values and external handles.

pub mod catalog;
pub use catalog::*;
pub mod builtins;
#[cfg(test)]
mod tests;
pub mod thread;

use std::collections::HashMap;
use std::sync;

pub use builtins::*;
pub use thread::*;

/// A typed value that crosses the execution boundary safely.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BoundaryType {
    Null,
    Bool,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bytes,
    Function,
    Array(Box<BoundaryType>),
    Tuple(Vec<BoundaryType>),
    Choice {
        variant: usize,
        payload: Option<Box<BoundaryType>>,
    },
    Handle {
        kind: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BoundaryValue {
    Null,
    Bool(bool),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    Bytes(Vec<u8>),
    Function {
        module_id: u32,
        func_idx: u16,
    },
    Array {
        element_type: BoundaryType,
        values: Vec<BoundaryValue>,
    },
    Tuple(Vec<BoundaryValue>),
    Choice {
        variant: usize, // Simplified from ChoiceVariantId
        payload: Option<Box<BoundaryValue>>,
    },
    Handle {
        proxy_module: Option<String>, // Set by Orchestrator upon future completion
        kind: String,                 // ExternalHandleKind
        id: u64,                      // ExternalHandleId
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryCodecError {
    TypeMismatch { expected: String, found: String },
    UnsupportedType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancellationOutcome {
    Confirmed,
    BestEffort,
    Unsupported,
    AlreadyCompleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionFailureKind {
    VmPanic,
    InvalidBytecode,
    MissingProvider,
    ProviderFailure,
    MissingAdapter,
    AdapterLoadFailure,
    ExternalSymbolFailure,
    BoundaryCodecFailure,
    InitializationFailure,
    Timeout,
    Cancelled,
    InvalidContinuation,
    DuplicateCompletion,
    DriverFailure,
    InternalRuntimeFailure,
}

/// A VM frame preserved across an asynchronous suspension boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionFrame {
    pub module_id: u64,
    pub function_id: u64,
    pub instruction_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionFailure {
    pub kind: ExecutionFailureKind,
    pub message: String,
    pub thread_id: Option<u64>,
    pub future_id: Option<u64>,
    pub request_id: Option<u64>,
    pub module_id: Option<u64>,
    pub function_id: Option<u64>,
    pub stack: Vec<ExecutionFrame>,
    pub cause: Option<Box<ExecutionFailure>>,
}

impl ExecutionFailure {
    pub fn new(kind: ExecutionFailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            thread_id: None,
            future_id: None,
            request_id: None,
            module_id: None,
            function_id: None,
            stack: vec![],
            cause: None,
        }
    }

    pub fn with_thread_id(mut self, thread_id: u64) -> Self {
        self.thread_id = Some(thread_id);
        self
    }

    pub fn with_module_id(mut self, module_id: u64) -> Self {
        self.module_id = Some(module_id);
        self
    }

    pub fn with_request_id(mut self, request_id: u64) -> Self {
        self.request_id = Some(request_id);
        self
    }

    pub fn with_future_id(mut self, future_id: u64) -> Self {
        self.future_id = Some(future_id);
        self
    }

    pub fn with_cause(mut self, cause: ExecutionFailure) -> Self {
        self.cause = Some(Box::new(cause));
        self
    }

    pub fn with_stack(mut self, stack: Vec<ExecutionFrame>) -> Self {
        self.stack = stack;
        self
    }
}

impl std::fmt::Display for ExecutionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for ExecutionFailure {}

pub trait MessageInjector: Send + Sync {
    fn inject_system_response(
        &self,
        thread_id: usize,
        request_id: u64,
        result: Result<BoundaryValue, ExecutionFailure>,
    );
}

pub trait HostProvider: Send {
    /// Declares the execution lane required to invoke this provider.
    ///
    /// Main-thread affinity is the safe default for host integrations that may
    /// touch platform APIs. Providers that are safe to transfer may opt into
    /// `TaskAffinity::Any`.
    fn affinity(&self, _name: &str) -> TaskAffinity {
        TaskAffinity::Main
    }

    fn dispatch(
        &mut self,
        thread_id: usize,
        request_id: u64,
        name: &str,
        args: &[BoundaryValue],
        injector: sync::Arc<dyn MessageInjector>,
    );

    /// Notifies the provider that a pending request no longer has an execution owner.
    fn cancel(&mut self, _thread_id: usize, _request_id: u64) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }
}

/// A bound external module invoked by Runtime on the main thread.
///
/// Implementations may create and coordinate arbitrary internal workers. Those workers must
/// report completion exclusively through the supplied `MessageInjector`.
pub trait BoundExternalModule: Send {
    fn dispatch(
        &mut self,
        symbol: &str,
        thread_id: usize,
        request_id: u64,
        args: &[BoundaryValue],
        injector: sync::Arc<dyn MessageInjector>,
    );

    fn cancel(
        &mut self,
        _symbol: &str,
        _thread_id: usize,
        _request_id: u64,
    ) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }

    /// Releases a foreign resource previously exposed through a nominal handle.
    fn release_handle(&mut self, _kind: &str, _id: u64) {}
}

/// External bindings are explicit and keyed by nominal proxy module.
#[derive(Default)]
pub struct ExternalBindings {
    modules: HashMap<String, Box<dyn BoundExternalModule>>,
    handles: std::collections::HashSet<(String, String, u64)>, // (proxy_module, kind, id)
}

impl ExternalBindings {
    pub fn register_module(
        &mut self,
        proxy_module: impl Into<String>,
        module: Box<dyn BoundExternalModule>,
    ) {
        self.modules.insert(proxy_module.into(), module);
    }

    pub fn get_mut(&mut self, proxy_module: &str) -> Option<&mut (dyn BoundExternalModule + '_)> {
        let module = self.modules.get_mut(proxy_module)?;
        Some(&mut **module)
    }

    /// Notifies the owning adapter that a request no longer has an execution owner.
    pub fn cancel(
        &mut self,
        proxy_module: &str,
        symbol: &str,
        thread_id: usize,
        request_id: u64,
    ) -> Option<CancellationOutcome> {
        self.get_mut(proxy_module)
            .map(|module| module.cancel(symbol, thread_id, request_id))
    }

    pub fn register_handle(
        &mut self,
        proxy_module: impl Into<String>,
        kind: impl Into<String>,
        id: u64,
    ) -> bool {
        let owner = proxy_module.into();
        if !self.modules.contains_key(&owner) {
            return false;
        }
        self.handles.insert((owner, kind.into(), id))
    }

    /// Atomically attaches every returned external handle to one adapter.
    /// A duplicate is rejected without registering any handle from the batch.
    pub fn register_handles(&mut self, proxy_module: &str, handles: &[(String, u64)]) -> bool {
        if !self.modules.contains_key(proxy_module) {
            return false;
        }
        let mut batch = std::collections::HashSet::new();
        if handles.iter().any(|(kind, id)| {
            !batch.insert((kind.clone(), *id))
                || self
                    .handles
                    .contains(&(proxy_module.to_string(), kind.clone(), *id))
        }) {
            if let Some(module) = self.modules.get_mut(proxy_module) {
                for (kind, id) in handles {
                    module.release_handle(kind, *id);
                }
            }
            return false;
        }
        for (kind, id) in handles {
            self.handles
                .insert((proxy_module.to_string(), kind.clone(), *id));
        }
        true
    }

    pub fn contains_handle(&self, proxy_module: &str, kind: &str, id: u64) -> bool {
        self.handles
            .contains(&(proxy_module.to_string(), kind.to_string(), id))
    }

    pub fn release_handle(&mut self, proxy_module: &str, kind: &str, id: u64) -> bool {
        if !self
            .handles
            .remove(&(proxy_module.to_string(), kind.to_string(), id))
        {
            return false;
        }
        if let Some(module) = self.get_mut(proxy_module) {
            module.release_handle(kind, id);
        }
        true
    }
}

/// Optional host capabilities supplied for one execution.
#[derive(Default)]
pub struct Providers {
    host: Option<Box<dyn HostProvider>>,
}

impl Providers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_host(host: Box<dyn HostProvider>) -> Self {
        Self { host: Some(host) }
    }

    pub fn host_mut(&mut self) -> Option<&mut (dyn HostProvider + 'static)> {
        self.host.as_deref_mut()
    }
}

pub type AdapterConfig = std::collections::BTreeMap<String, AdapterConfigValue>;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum AdapterConfigValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<AdapterConfigValue>),
    Table(AdapterConfig),
}

/// Description of an external proxy module compiled from a .gfp file.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExternalModuleDescriptor {
    pub adapter: String,
    pub config: AdapterConfig,
    pub exports: Vec<ExternalFunctionSignature>,
}

/// A declarative external-module dependency produced during compilation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExternalModuleRequirement {
    pub proxy_module: String,
    pub descriptor: ExternalModuleDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExternalFunctionSignature {
    pub name: String,
    pub is_async: bool,
    pub parameter_types: Vec<BoundaryType>,
    pub return_type: BoundaryType,
}

#[derive(Debug, thiserror::Error)]
pub enum AdapterValidationError {
    #[error("unsupported adapter: {0}")]
    UnsupportedAdapter(String),

    #[error("missing configuration target for platform '{platform}': {reason}")]
    MissingPlatformTarget { platform: String, reason: String },

    #[error("invalid schema: {0}")]
    InvalidSchema(String),
}

#[derive(Debug, thiserror::Error)]
#[error("adapter load error [{code}]: {message}")]
pub struct AdapterLoadError {
    pub code: String,
    pub message: String,
}

/// Development-time validation for an external proxy descriptor.
pub trait ExternalAdapterSchema: Send + Sync {
    fn name(&self) -> &str;
    /// Complete declarative adapter schema used for catalog identity.
    ///
    /// This must change whenever adapter functions, parameter or return types,
    /// modifiers, targets, or other validation-relevant semantics change.
    fn catalog_schema(&self) -> String;
    fn validate_schema(
        &self,
        descriptor: &ExternalModuleDescriptor,
    ) -> Result<(), AdapterValidationError>;
}

pub struct ExternalLoadContext {
    pub properties: std::collections::BTreeMap<String, String>,
}

/// Optional package-time loader. Runtime receives only [`ExternalBindings`].
pub trait ExternalModuleLoader: Send + Sync {
    fn load_module(
        &self,
        requirement: &ExternalModuleRequirement,
        context: &ExternalLoadContext,
    ) -> Result<Box<dyn BoundExternalModule>, AdapterLoadError>;
}

/// Compatibility composition for hosts that provide both development contracts.
#[deprecated(
    note = "Adapter schema and concrete loaders are now strictly separated. Use
`CapabilityCatalog` for schema validation during `Workspace::compile` and provide
`ExternalModuleLoader` during bootstrap/preflight."
)]
pub trait ModuleAdapter: ExternalAdapterSchema + ExternalModuleLoader {}

#[allow(deprecated)]
impl<T> ModuleAdapter for T where T: ExternalAdapterSchema + ExternalModuleLoader {}
