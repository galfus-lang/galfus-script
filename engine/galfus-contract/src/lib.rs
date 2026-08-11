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
pub mod version;

use std::collections::HashMap;
use std::sync;

use galfus_core::{BindingId, HandleId, OpaqueTypeId};

pub use builtins::*;
pub use thread::*;
pub use version::*;

/// A typed value that crosses the execution boundary safely.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
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
    Nullable(Box<BoundaryType>),
    Tuple(Vec<BoundaryType>),
    Choice {
        variant: u32,
        payload: Option<Box<BoundaryType>>,
    },
    Handle {
        type_id: OpaqueTypeId,
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
        variant: u32, // Simplified from ChoiceVariantId
        payload: Option<Box<BoundaryValue>>,
    },
    Handle {
        type_id: OpaqueTypeId,
        binding_id: Option<BindingId>, // Set by Orchestrator upon future completion
        id: HandleId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryCodecError {
    TypeMismatch { expected: String, found: String },
    UnsupportedType,
    HeapExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancellationOutcome {
    Confirmed,
    BestEffort,
    Unsupported,
    AlreadyCompleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleReleaseOutcome {
    Released,
    AlreadyReleased,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("adapter handle release failed [{code}]: {message}")]
pub struct AdapterReleaseError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionFailureKind {
    VmPanic,
    InvalidBytecode,
    MissingProvider,
    ProviderFailure,
    MissingAdapter,
    AdapterLoadFailure,
    AdapterCallFailure,
    BoundaryCodecFailure,
    HeapExhausted,
    InitializationFailure,
    Timeout,
    Cancelled,
    InvalidContinuation,
    DuplicateThreadKey,
    DuplicateCompletion,
    DriverFailure,
    InternalRuntimeFailure,
    IdSpaceExhausted,
}

/// A VM frame preserved across an asynchronous suspension boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionFrame {
    pub module_id: u64,
    pub function_id: u64,
    pub instruction_offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionFailure {
    pub kind: ExecutionFailureKind,
    pub message: String,
    pub thread_id: Option<galfus_core::ThreadId>,
    pub future_id: Option<galfus_core::FutureId>,
    pub request_id: Option<galfus_core::RequestId>,
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

    pub fn with_thread_id(mut self, thread_id: galfus_core::ThreadId) -> Self {
        self.thread_id = Some(thread_id);
        self
    }

    pub fn with_module_id(mut self, module_id: u64) -> Self {
        self.module_id = Some(module_id);
        self
    }

    pub fn with_request_lease(mut self, request_lease: galfus_core::RequestLease) -> Self {
        self.request_id = Some(request_lease.id);
        self
    }

    pub fn with_future_id(mut self, future_id: galfus_core::FutureId) -> Self {
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
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        result: Result<BoundaryValue, ExecutionFailure>,
    );
}

pub trait HostProvider: Send {
    /// Immutable declaration of every provider module and operation implemented by this host.
    fn descriptor(&self) -> ProviderDescriptor;

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
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[BoundaryValue],
        injector: sync::Arc<dyn MessageInjector>,
    );

    /// Notifies the provider that a pending request no longer has an execution owner.
    fn cancel(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
    ) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }
}

/// A bound external module invoked by Runtime on the main thread.
///
/// Implementations may create and coordinate arbitrary internal workers. Those workers must
/// report completion exclusively through the supplied `MessageInjector`.
pub trait AdapterModuleBinding: Send {
    /// Immutable declaration of the adapter proxy surface implemented by this binding.
    fn descriptor(&self) -> AdapterModuleDescriptor;

    fn dispatch(
        &mut self,
        symbol: &str,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        args: &[BoundaryValue],
        injector: sync::Arc<dyn MessageInjector>,
    );

    fn cancel(
        &mut self,
        _symbol: &str,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
    ) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }

    /// Releases a foreign resource previously exposed through a nominal handle.
    fn release_handle(
        &mut self,
        _type_id: &OpaqueTypeId,
        _id: HandleId,
    ) -> Result<HandleReleaseOutcome, AdapterReleaseError> {
        Ok(HandleReleaseOutcome::Released)
    }
}

/// Adapter bindings are explicit and keyed by nominal proxy module.
#[derive(Default)]
pub struct AdapterBindings {
    modules: HashMap<String, AdapterBinding>,
    handles: std::collections::HashSet<(BindingId, OpaqueTypeId, HandleId)>,
    binding_id_manager: galfus_core::id_manager::IdManager<BindingId>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdapterBindingError {
    #[error("adapter binding for proxy module `{0}` is already registered")]
    DuplicateProxyModule(String),
    #[error("id space for domain `{domain}` is exhausted")]
    IdSpaceExhausted { domain: &'static str },
    #[error("handle is invalid or duplicated")]
    InvalidHandle,
    #[error("cannot remove binding with active handles")]
    HandlesStillActive,
    #[error("could not compensate a rejected handle batch: {0}")]
    CompensationReleaseFailed(AdapterBindingReleaseError),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdapterBindingReleaseError {
    #[error("adapter binding registry is poisoned")]
    RegistryPoisoned,
    #[error("adapter binding {binding_id:?} could not release handle {type_id:?}/{id:?}: {error}")]
    AdapterReleaseFailed {
        binding_id: BindingId,
        type_id: OpaqueTypeId,
        id: HandleId,
        error: AdapterReleaseError,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdapterBindingsCloseReport {
    pub released: usize,
    pub already_released: usize,
    pub failures: Vec<AdapterBindingReleaseError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterHandleRelease {
    proxy_module: String,
    binding_id: BindingId,
    type_id: OpaqueTypeId,
    id: HandleId,
}

impl AdapterHandleRelease {
    pub fn proxy_module(&self) -> &str {
        self.proxy_module.as_str()
    }

    pub fn binding_id(&self) -> BindingId {
        self.binding_id
    }

    pub fn type_id(&self) -> &OpaqueTypeId {
        &self.type_id
    }

    pub fn id(&self) -> HandleId {
        self.id
    }
}

impl AdapterBindingsCloseReport {
    pub fn is_complete(&self) -> bool {
        self.failures.is_empty()
    }
}

struct AdapterBinding {
    id: BindingId,
    next_handle_id: Option<HandleId>,
    module: Option<Box<dyn AdapterModuleBinding>>,
}

impl AdapterBindings {
    pub fn register_module(
        &mut self,
        proxy_module: impl Into<String>,
        module: Box<dyn AdapterModuleBinding>,
    ) -> Result<BindingId, AdapterBindingError> {
        let proxy_module = proxy_module.into();
        if self.modules.contains_key(&proxy_module) {
            return Err(AdapterBindingError::DuplicateProxyModule(proxy_module));
        }
        let id = self.binding_id_manager.try_allocate().ok_or(
            AdapterBindingError::IdSpaceExhausted {
                domain: "BindingId",
            },
        )?;
        self.modules.insert(
            proxy_module,
            AdapterBinding {
                id,
                next_handle_id: Some(HandleId::new(1)),
                module: Some(module),
            },
        );
        Ok(id)
    }

    pub fn remove_binding(&mut self, proxy_module: &str) -> Result<(), AdapterBindingError> {
        let Some(binding) = self.modules.get(proxy_module) else {
            return Ok(());
        };
        let id = binding.id;
        let has_handles = self.handles.iter().any(|(b_id, _, _)| *b_id == id);
        if has_handles {
            return Err(AdapterBindingError::HandlesStillActive);
        }
        self.modules.remove(proxy_module);
        self.binding_id_manager.free(id);
        Ok(())
    }

    pub fn has_module(&self, proxy_module: &str) -> bool {
        self.modules
            .get(proxy_module)
            .is_some_and(|binding| binding.module.is_some())
    }

    pub fn get_mut(&mut self, proxy_module: &str) -> Option<&mut Box<dyn AdapterModuleBinding>> {
        self.modules.get_mut(proxy_module)?.module.as_mut()
    }

    /// Temporarily removes one adapter from the registry for an external callback.
    ///
    /// Callers must restore it with [`Self::restore_module`] after the callback finishes.
    pub fn take_module(&mut self, proxy_module: &str) -> Option<Box<dyn AdapterModuleBinding>> {
        self.modules.get_mut(proxy_module)?.module.take()
    }

    pub fn restore_module(
        &mut self,
        proxy_module: &str,
        module: Box<dyn AdapterModuleBinding>,
    ) -> Result<(), AdapterBindingError> {
        let Some(binding) = self.modules.get_mut(proxy_module) else {
            return Err(AdapterBindingError::InvalidHandle);
        };
        if binding.module.is_some() {
            return Err(AdapterBindingError::DuplicateProxyModule(
                proxy_module.to_string(),
            ));
        }
        binding.module = Some(module);
        Ok(())
    }

    pub fn binding_id(&self, proxy_module: &str) -> Option<BindingId> {
        self.modules.get(proxy_module).map(|binding| binding.id)
    }

    pub fn validates(&self, requirement: &AdapterModuleRequirement) -> bool {
        self.modules
            .get(requirement.proxy_module.as_str())
            .and_then(|binding| binding.module.as_ref())
            .is_some_and(|module| module.descriptor() == requirement.descriptor)
    }

    /// Notifies the owning adapter that a request no longer has an execution owner.
    pub fn cancel(
        &mut self,
        proxy_module: &str,
        symbol: &str,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
    ) -> Option<CancellationOutcome> {
        self.modules
            .get_mut(proxy_module)
            .and_then(|binding| binding.module.as_deref_mut())
            .map(|module| module.cancel(symbol, thread_id, request_lease))
    }

    pub fn register_handle(
        &mut self,
        binding_id: BindingId,
        type_id: OpaqueTypeId,
        id: HandleId,
    ) -> Result<(), AdapterBindingError> {
        self.register_handles(binding_id, &[(type_id, id)])
    }

    /// Atomically attaches every returned external handle to one adapter.
    /// A duplicate is rejected without registering any handle from the batch.
    pub fn register_handles(
        &mut self,
        binding_id: BindingId,
        handles: &[(OpaqueTypeId, HandleId)],
    ) -> Result<(), AdapterBindingError> {
        let Some(proxy_module) = self.proxy_module_for(binding_id).map(str::to_string) else {
            return Err(AdapterBindingError::InvalidHandle);
        };
        let Some(binding) = self
            .modules
            .values()
            .find(|binding| binding.id == binding_id)
        else {
            return Err(AdapterBindingError::InvalidHandle);
        };
        let mut next_handle_id = binding.next_handle_id;
        let mut compensation = Vec::new();
        let mut invalid = false;
        let mut exhausted = false;

        for (type_id, id) in handles {
            let Some(expected_id) = next_handle_id else {
                exhausted = true;
                continue;
            };
            let handle_key = (binding_id, type_id.clone(), *id);
            if !type_id_belongs_to_proxy(type_id, &proxy_module)
                || *id != expected_id
                || self.handles.contains(&handle_key)
            {
                invalid = true;
                continue;
            }
            compensation.push((type_id.clone(), *id));
            next_handle_id = id.raw().checked_add(1).map(HandleId::new);
        }

        if exhausted || invalid {
            if let Some(binding) = self
                .modules
                .values_mut()
                .find(|binding| binding.id == binding_id)
            {
                for (type_id, id) in compensation {
                    let Some(module) = binding.module.as_deref_mut() else {
                        return Err(AdapterBindingError::InvalidHandle);
                    };
                    module.release_handle(&type_id, id).map_err(|error| {
                        AdapterBindingError::CompensationReleaseFailed(
                            AdapterBindingReleaseError::AdapterReleaseFailed {
                                binding_id,
                                type_id,
                                id,
                                error,
                            },
                        )
                    })?;
                }
            }
            if exhausted {
                return Err(AdapterBindingError::IdSpaceExhausted { domain: "HandleId" });
            }
            return Err(AdapterBindingError::InvalidHandle);
        }
        let Some(binding) = self
            .modules
            .values_mut()
            .find(|binding| binding.id == binding_id)
        else {
            return Err(AdapterBindingError::InvalidHandle);
        };
        binding.next_handle_id = next_handle_id;
        for (type_id, id) in handles {
            self.handles.insert((binding_id, type_id.clone(), *id));
        }
        Ok(())
    }

    pub fn contains_handle(
        &self,
        binding_id: BindingId,
        type_id: &OpaqueTypeId,
        id: HandleId,
    ) -> bool {
        self.handles.contains(&(binding_id, type_id.clone(), id))
    }

    pub fn active_handles(&self) -> Vec<(BindingId, OpaqueTypeId, HandleId)> {
        let mut handles = self.handles.iter().cloned().collect::<Vec<_>>();
        handles.sort_unstable();
        handles
    }

    /// Removes local ownership before an external release callback.
    pub fn take_handle_for_release(
        &mut self,
        binding_id: BindingId,
        type_id: &OpaqueTypeId,
        id: HandleId,
    ) -> Option<AdapterHandleRelease> {
        let proxy_module = self.proxy_module_for(binding_id)?.to_string();
        self.handles
            .remove(&(binding_id, type_id.clone(), id))
            .then_some(AdapterHandleRelease {
                proxy_module,
                binding_id,
                type_id: type_id.clone(),
                id,
            })
    }

    pub fn restore_handle_after_failed_release(&mut self, release: AdapterHandleRelease) {
        self.handles
            .insert((release.binding_id, release.type_id, release.id));
    }

    pub fn release_handle(
        &mut self,
        binding_id: BindingId,
        type_id: &OpaqueTypeId,
        id: HandleId,
    ) -> Result<HandleReleaseOutcome, AdapterBindingReleaseError> {
        let Some(binding) = self
            .modules
            .values_mut()
            .find(|binding| binding.id == binding_id)
        else {
            return Ok(HandleReleaseOutcome::AlreadyReleased);
        };
        if !self.handles.remove(&(binding_id, type_id.clone(), id)) {
            return Ok(HandleReleaseOutcome::AlreadyReleased);
        }
        let Some(module) = binding.module.as_deref_mut() else {
            self.handles.insert((binding_id, type_id.clone(), id));
            return Ok(HandleReleaseOutcome::AlreadyReleased);
        };
        match module.release_handle(type_id, id) {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                self.handles.insert((binding_id, type_id.clone(), id));
                Err(AdapterBindingReleaseError::AdapterReleaseFailed {
                    binding_id,
                    type_id: type_id.clone(),
                    id,
                    error,
                })
            }
        }
    }

    /// Releases every foreign handle still owned by this execution.
    pub fn close(&mut self) -> AdapterBindingsCloseReport {
        let mut handles = self.handles.iter().cloned().collect::<Vec<_>>();
        handles.sort_unstable();
        let mut report = AdapterBindingsCloseReport::default();
        for (binding_id, type_id, id) in handles {
            match self.release_handle(binding_id, &type_id, id) {
                Ok(HandleReleaseOutcome::Released) => report.released += 1,
                Ok(HandleReleaseOutcome::AlreadyReleased) => report.already_released += 1,
                Err(error) => report.failures.push(error),
            }
        }
        report
    }

    fn proxy_module_for(&self, binding_id: BindingId) -> Option<&str> {
        self.modules.iter().find_map(|(proxy_module, binding)| {
            (binding.id == binding_id).then_some(proxy_module.as_str())
        })
    }
}

fn type_id_belongs_to_proxy(type_id: &OpaqueTypeId, proxy_module: &str) -> bool {
    type_id.proxy_module() == proxy_module.trim_end_matches(".gfp")
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

    /// Temporarily removes the host provider for an external callback.
    ///
    /// The caller must restore it after dispatch or cancellation completes.
    pub fn take_host(&mut self) -> Option<Box<dyn HostProvider>> {
        self.host.take()
    }

    pub fn restore_host(&mut self, host: Box<dyn HostProvider>) {
        self.host = Some(host);
    }

    pub fn host_mut(&mut self) -> Option<&mut (dyn HostProvider + 'static)> {
        self.host.as_deref_mut()
    }

    pub fn validates(&self, requirement: &ProviderModuleRequirement) -> bool {
        self.host
            .as_deref()
            .is_some_and(|host| host.descriptor().validates(requirement))
    }
}

/// Mutable bootstrap state for one runtime capability set.
#[derive(Default)]
pub struct RuntimeCapabilitiesBuilder {
    providers: Option<Providers>,
    adapter_bindings: AdapterBindings,
}

impl RuntimeCapabilitiesBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_providers(mut self, providers: Providers) -> Self {
        self.providers = Some(providers);
        self
    }

    pub fn with_adapter_bindings(mut self, adapter_bindings: AdapterBindings) -> Self {
        self.adapter_bindings = adapter_bindings;
        self
    }

    pub fn register_adapter(
        &mut self,
        proxy_module: impl Into<String>,
        module: Box<dyn AdapterModuleBinding>,
    ) -> Result<BindingId, AdapterBindingError> {
        self.adapter_bindings.register_module(proxy_module, module)
    }

    /// Consumes construction state and publishes immutable capability topology.
    pub fn build(self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            providers: self
                .providers
                .map(|providers| sync::Arc::new(sync::Mutex::new(providers))),
            adapter_bindings: sync::Arc::new(sync::Mutex::new(self.adapter_bindings)),
        }
    }
}

/// Frozen capability topology for one runtime execution.
pub struct RuntimeCapabilities {
    providers: Option<sync::Arc<sync::Mutex<Providers>>>,
    adapter_bindings: sync::Arc<sync::Mutex<AdapterBindings>>,
}

impl RuntimeCapabilities {
    pub fn builder() -> RuntimeCapabilitiesBuilder {
        RuntimeCapabilitiesBuilder::new()
    }

    /// Transfers the frozen capabilities to a single runtime execution.
    pub fn into_runtime_handles(
        self,
    ) -> (
        Option<sync::Arc<sync::Mutex<Providers>>>,
        sync::Arc<sync::Mutex<AdapterBindings>>,
    ) {
        (self.providers, self.adapter_bindings)
    }
}

pub type AdapterConfig = std::collections::BTreeMap<String, AdapterConfigValue>;

/// SHA-256 digest used to identify immutable package and adapter content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    pub fn of(content: &[u8]) -> Self {
        use sha2::{Digest, Sha256};

        Self(Sha256::digest(content).into())
    }

    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Opaque execution target identity shared by a package and its execution host.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ExecutionTarget(String);

impl ExecutionTarget {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.trim().is_empty()).then_some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// One adapter-defined artifact candidate for an execution target.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AdapterTarget {
    pub target: ExecutionTarget,
    pub locator: String,
    pub platform: String,
    pub abi: String,
    pub artifact: AdapterArtifact,
}

/// Immutable artifact metadata declared by an adapter target.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AdapterArtifact {
    pub content_hash: ContentHash,
    pub size_bytes: u64,
    pub media_type: String,
    pub content_version: galfus_core::Version,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdapterArtifactIntegrityError {
    #[error("adapter artifact size is {actual} bytes, expected {expected}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("adapter artifact content hash is {actual}, expected {expected}")]
    HashMismatch {
        expected: ContentHash,
        actual: ContentHash,
    },
}

impl AdapterArtifact {
    pub fn verify(
        &self,
        content: Vec<u8>,
    ) -> Result<VerifiedAdapterArtifact, AdapterArtifactIntegrityError> {
        let actual_size = content.len() as u64;
        if actual_size != self.size_bytes {
            return Err(AdapterArtifactIntegrityError::SizeMismatch {
                expected: self.size_bytes,
                actual: actual_size,
            });
        }

        let actual_hash = ContentHash::of(&content);
        if actual_hash != self.content_hash {
            return Err(AdapterArtifactIntegrityError::HashMismatch {
                expected: self.content_hash,
                actual: actual_hash,
            });
        }

        Ok(VerifiedAdapterArtifact { content })
    }
}

/// Artifact bytes verified against the immutable metadata in a package image.
pub struct VerifiedAdapterArtifact {
    content: Vec<u8>,
}

impl VerifiedAdapterArtifact {
    pub fn as_bytes(&self) -> &[u8] {
        self.content.as_slice()
    }
}

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

/// Description of an adapter proxy module compiled from a .gfp file.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AdapterModuleDescriptor {
    pub adapter: String,
    pub config: AdapterConfig,
    pub targets: Vec<AdapterTarget>,
    pub exports: Vec<AdapterFunctionSignature>,
}

impl AdapterModuleDescriptor {
    pub fn empty() -> Self {
        Self {
            adapter: String::new(),
            config: AdapterConfig::new(),
            targets: Vec::new(),
            exports: Vec::new(),
        }
    }

    /// Orders schema exports so equivalent adapter declarations have one package representation.
    pub fn canonicalize(&mut self) {
        self.exports.sort();
        self.targets.sort_by(|left, right| {
            (
                left.target.as_str(),
                left.platform.as_str(),
                left.abi.as_str(),
                left.locator.as_str(),
            )
                .cmp(&(
                    right.target.as_str(),
                    right.platform.as_str(),
                    right.abi.as_str(),
                    right.locator.as_str(),
                ))
        });
    }
}

/// A declarative external-module dependency produced during compilation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AdapterModuleRequirement {
    pub proxy_module: String,
    pub descriptor: AdapterModuleDescriptor,
    pub boundary_abi: BoundaryAbiVersion,
}

/// The target and locator selected by preflight for one adapter proxy module.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SelectedAdapterTarget {
    pub proxy_module: String,
    pub target: AdapterTarget,
    pub boundary_abi: BoundaryAbiVersion,
}

/// A provider schema required by a package image.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderModuleRequirement {
    pub module_path: String,
    pub schema_fingerprint: u64,
    pub boundary_abi: BoundaryAbiVersion,
    pub exports: Vec<ProviderFunctionSignature>,
}

/// Concrete provider surface for one declarative bridge module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderModuleDescriptor {
    pub module_path: String,
    pub schema_fingerprint: u64,
    pub boundary_abi: BoundaryAbiVersion,
    pub exports: Vec<ProviderFunctionSignature>,
}

/// Immutable provider capability table supplied by one host.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProviderDescriptor {
    pub modules: Vec<ProviderModuleDescriptor>,
}

impl ProviderDescriptor {
    pub fn validates(&self, requirement: &ProviderModuleRequirement) -> bool {
        self.modules.iter().any(|module| {
            module.module_path == requirement.module_path
                && module.schema_fingerprint == requirement.schema_fingerprint
                && module.boundary_abi == requirement.boundary_abi
                && module.exports == requirement.exports
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct ProviderFunctionSignature {
    pub name: String,
    pub parameter_types: Vec<BoundaryType>,
    pub return_type: BoundaryType,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct AdapterFunctionSignature {
    pub name: String,
    pub is_async: bool,
    pub parameter_types: Vec<BoundaryType>,
    pub return_type: BoundaryType,
}

#[derive(Debug, thiserror::Error)]
pub enum AdapterValidationError {
    #[error("unsupported adapter: {0}")]
    UnsupportedAdapter(String),

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
pub trait AdapterSchema: Send + Sync {
    fn name(&self) -> &str;
    /// Complete declarative adapter schema used for catalog identity.
    ///
    /// This must change whenever adapter functions, parameter or return types,
    /// modifiers, or other validation-relevant semantics change.
    fn catalog_schema(&self) -> String;
    fn validate_schema(
        &self,
        descriptor: &AdapterModuleDescriptor,
    ) -> Result<(), AdapterValidationError>;
}

pub struct AdapterLoadContext {
    pub target: ExecutionTarget,
    pub properties: std::collections::BTreeMap<String, String>,
}

/// Optional package-time loader. Runtime receives only [`AdapterBindings`].
pub trait AdapterModuleLoader: Send + Sync {
    /// Resolves the selected artifact. Preflight verifies the returned bytes before binding.
    fn load_artifact(
        &self,
        selected_target: &SelectedAdapterTarget,
        context: &AdapterLoadContext,
    ) -> Result<Vec<u8>, AdapterLoadError>;

    fn load_module(
        &self,
        requirement: &AdapterModuleRequirement,
        selected_target: &SelectedAdapterTarget,
        artifact: VerifiedAdapterArtifact,
        context: &AdapterLoadContext,
    ) -> Result<Box<dyn AdapterModuleBinding>, AdapterLoadError>;
}
