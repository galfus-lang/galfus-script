#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ModuleId(u32);

impl ModuleId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

impl crate::id_manager::RawId for ModuleId {
    fn new(raw: u32) -> Self {
        Self(raw)
    }
    fn raw(&self) -> u32 {
        self.0
    }
}

/// Nominal identity of an opaque type exported by one adapter proxy module.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct OpaqueTypeId {
    proxy_module: String,
    name: String,
}

impl OpaqueTypeId {
    pub fn new(proxy_module: impl Into<String>, name: impl Into<String>) -> Option<Self> {
        let proxy_module = proxy_module.into();
        let name = name.into();
        if proxy_module.trim().is_empty() || name.trim().is_empty() {
            return None;
        }
        Some(Self { proxy_module, name })
    }

    pub fn proxy_module(&self) -> &str {
        self.proxy_module.as_str()
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }
}

/// Immutable identity assigned to one adapter binding during bootstrap.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct BindingId(u32);

impl BindingId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl crate::id_manager::RawId for BindingId {
    fn new(raw: u32) -> Self {
        Self(raw)
    }
    fn raw(&self) -> u32 {
        self.0
    }
}

/// Identifies a unique thread of execution within the virtual kernel.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ThreadId(u32);

impl ThreadId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl crate::id_manager::RawId for ThreadId {
    fn new(raw: u32) -> Self {
        Self(raw)
    }
    fn raw(&self) -> u32 {
        self.0
    }
}

/// Identifies a pending external request dispatched to a host provider or adapter.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct RequestId(u32);

impl RequestId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl crate::id_manager::RawId for RequestId {
    fn new(raw: u32) -> Self {
        Self(raw)
    }
    fn raw(&self) -> u32 {
        self.0
    }
}

/// Identifies an asynchronous future managed by the Orchestrator.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct FutureId(u32);

impl FutureId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl crate::id_manager::RawId for FutureId {
    fn new(raw: u32) -> Self {
        Self(raw)
    }
    fn raw(&self) -> u32 {
        self.0
    }
}

/// Identifies an active timeout in the BlockedQueue.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct TimerId(u32);

impl TimerId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl crate::id_manager::RawId for TimerId {
    fn new(raw: u32) -> Self {
        Self(raw)
    }
    fn raw(&self) -> u32 {
        self.0
    }
}

/// Identifies one aggregate future wait within an Orchestrator.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct CoordinatorId(u32);

impl CoordinatorId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl crate::id_manager::RawId for CoordinatorId {
    fn new(raw: u32) -> Self {
        Self(raw)
    }
    fn raw(&self) -> u32 {
        self.0
    }
}

/// Public, non-reusable resource identity within one adapter binding.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct HandleId(u32);

impl HandleId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl crate::id_manager::RawId for HandleId {
    fn new(raw: u32) -> Self {
        Self(raw)
    }
    fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(u32);

impl SourceId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

impl crate::id_manager::RawId for SourceId {
    fn new(raw: u32) -> Self {
        Self(raw)
    }
    fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u32);

impl NodeId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolId(u32);

impl SymbolId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(u32);

impl ScopeId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(u32);

impl TypeId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionId(u32);

impl FunctionId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StructId(u32);

impl StructId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnumId(u32);

impl EnumId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChoiceId(u32);

impl ChoiceId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConstraintId(u32);

impl ConstraintId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExportId(u32);

impl ExportId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImportId(u32);

impl ImportId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Revision(pub u64);

impl Revision {
    pub const fn new(rev: u64) -> Self {
        Self(rev)
    }

    pub fn next(&mut self) {
        self.0 += 1;
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct SemanticRevision(pub u64);

impl SemanticRevision {
    pub const fn new(rev: u64) -> Self {
        Self(rev)
    }

    pub fn next(&mut self) {
        self.0 += 1;
    }
}
