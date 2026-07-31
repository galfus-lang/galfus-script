//! Host integration contracts for Galfus execution.

#[cfg(test)]
mod tests;
pub mod thread;

use std::collections::HashMap;
use std::sync;

pub use thread::*;

/// A typed value that crosses the execution boundary safely.
#[derive(Debug, Clone, PartialEq, Eq)]
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
        kind: String, // ExternalHandleKind
        id: u64,      // ExternalHandleId
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryCodecError {
    TypeMismatch { expected: String, found: String },
    UnsupportedType,
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
    fn cancel(&mut self, _thread_id: usize, _request_id: u64) {}
}

/// Typed foreign-function integration for one nominal adapter symbol.
pub trait HostAdapter: Send {
    fn affinity(&self) -> TaskAffinity {
        TaskAffinity::Main
    }

    fn dispatch(
        &mut self,
        thread_id: usize,
        request_id: u64,
        args: &[BoundaryValue],
        injector: sync::Arc<dyn MessageInjector>,
    );

    fn cancel(&mut self, _thread_id: usize, _request_id: u64) {}

    /// Releases a foreign resource previously exposed through a nominal handle.
    fn release_handle(&mut self, _kind: &str, _id: u64) {}
}

/// Adapter ownership is explicit and keyed by its nominal module and symbol.
#[derive(Default)]
pub struct Adapters {
    entries: HashMap<(String, String), Box<dyn HostAdapter>>,
    handles: HashMap<(String, u64), (String, String)>,
}

impl Adapters {
    pub fn register(
        &mut self,
        module: impl Into<String>,
        symbol: impl Into<String>,
        adapter: Box<dyn HostAdapter>,
    ) {
        self.entries.insert((module.into(), symbol.into()), adapter);
    }

    pub fn get_mut(&mut self, module: &str, symbol: &str) -> Option<&mut (dyn HostAdapter + '_)> {
        let adapter = self
            .entries
            .get_mut(&(module.to_string(), symbol.to_string()))?;
        Some(&mut **adapter)
    }

    /// Notifies the owning adapter that a request no longer has an execution owner.
    pub fn cancel(&mut self, module: &str, symbol: &str, thread_id: usize, request_id: u64) {
        if let Some(adapter) = self.get_mut(module, symbol) {
            adapter.cancel(thread_id, request_id);
        }
    }

    pub fn register_handle(
        &mut self,
        module: impl Into<String>,
        symbol: impl Into<String>,
        kind: impl Into<String>,
        id: u64,
    ) -> bool {
        let owner = (module.into(), symbol.into());
        if !self.entries.contains_key(&owner) {
            return false;
        }
        self.handles.insert((kind.into(), id), owner).is_none()
    }

    pub fn contains_handle(&self, kind: &str, id: u64) -> bool {
        self.handles.contains_key(&(kind.to_string(), id))
    }

    pub fn release_handle(&mut self, kind: &str, id: u64) -> bool {
        let Some((module, symbol)) = self.handles.remove(&(kind.to_string(), id)) else {
            return false;
        };
        if let Some(adapter) = self.get_mut(&module, &symbol) {
            adapter.release_handle(kind, id);
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
