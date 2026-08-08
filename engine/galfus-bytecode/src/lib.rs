// =========================================================================
// Constant Pool
// =========================================================================

pub mod graph;
pub mod graph_resolver;
pub mod instruction;
pub mod opcode;
pub mod validation;

pub use graph::{
    BytecodeGraph, BytecodeGraphTransaction, BytecodeGraphTransactionError,
    BytecodeGraphValidationError, BytecodeGraphValidationErrors, BytecodeNode, ImportEdge,
};
pub use graph_resolver::{GraphResolutionError, ModuleImports, ResolvedImport};
pub use instruction::*;
pub use opcode::*;
pub use validation::*;

/// Version of the bytecode instruction set and in-memory layout.
///
/// This is independent from [`BytecodeGraph::version`], which only identifies
/// the ordering of graph snapshots within one compilation session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BytecodeFormatVersion(u16);

impl BytecodeFormatVersion {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// The only bytecode format this runtime release can interpret.
pub const CURRENT_BYTECODE_FORMAT_VERSION: BytecodeFormatVersion = BytecodeFormatVersion::new(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BytecodeFormatError {
    #[error("legacy bytecode format version {actual:?}; supported version is {supported:?}")]
    LegacyVersion {
        supported: BytecodeFormatVersion,
        actual: BytecodeFormatVersion,
    },
    #[error("future bytecode format version {actual:?}; supported version is {supported:?}")]
    FutureVersion {
        supported: BytecodeFormatVersion,
        actual: BytecodeFormatVersion,
    },
}

pub fn validate_bytecode_format(actual: BytecodeFormatVersion) -> Result<(), BytecodeFormatError> {
    match actual.raw().cmp(&CURRENT_BYTECODE_FORMAT_VERSION.raw()) {
        std::cmp::Ordering::Less => Err(BytecodeFormatError::LegacyVersion {
            supported: CURRENT_BYTECODE_FORMAT_VERSION,
            actual,
        }),
        std::cmp::Ordering::Equal => Ok(()),
        std::cmp::Ordering::Greater => Err(BytecodeFormatError::FutureVersion {
            supported: CURRENT_BYTECODE_FORMAT_VERSION,
            actual,
        }),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Constant {
    Bool(bool),
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    Uint8(u8),
    Uint16(u16),
    Uint32(u32),
    Uint64(u64),
    Float32(f32),
    Float64(f64),
    String(String),
    Bytes(Vec<u8>),
    Function(FuncIdx),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConstantPool {
    pub constants: Vec<Constant>,
}

// =========================================================================
// Types & Layout Table
// =========================================================================

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BytecodeType {
    Null,
    Bool,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Float32,
    Float64,
    AdapterHandle(String),
    Struct(StructLayoutIdx),
    Array(TypeIdx),
    Nullable(TypeIdx),
    Tuple(Vec<TypeIdx>),
    Choice(ChoiceLayoutIdx),
    Constraint(String),
    Function { params: Vec<TypeIdx>, ret: TypeIdx },
    ChoiceVariant(ChoiceLayoutIdx, u16),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum OwnershipKind {
    Strong,
    Weak,
    Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldLayout {
    pub name: String,
    pub ty: TypeIdx,
    pub offset: usize,
    pub ownership: OwnershipKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructLayout {
    pub name: String,
    pub fields: Vec<FieldLayout>,
    pub constraints: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceVariantLayout {
    pub name: String,
    pub payload_ty: Option<TypeIdx>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceLayout {
    pub name: String,
    pub variants: Vec<ChoiceVariantLayout>,
}

// =========================================================================
// Imports & Exports
// =========================================================================

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportKind {
    Function,
    Global,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportSlot {
    pub module_name: String,
    pub symbol_name: String,
    pub ty: TypeIdx,
    pub kind: ImportKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportKind {
    Function(FuncIdx),
    Global(GlobalIdx),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportSlot {
    pub symbol_name: String,
    pub kind: ExportKind,
}

// =========================================================================
// Bytecode Function & Module Container
// =========================================================================

#[derive(Clone, Debug, PartialEq)]
pub struct AdapterProxyMetadata {
    pub proxy_module: String,
    pub symbol: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BytecodeFunction {
    pub name: String,
    pub param_count: u8,
    pub local_count: u16,
    pub temp_count: u16,
    pub return_ty: TypeIdx,
    pub adapter_proxy_metadata: Option<AdapterProxyMetadata>,
    pub instructions: Vec<Instruction>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BytecodeModule {
    pub name: String,
    /// Number of addressable global slots owned by this module.
    pub global_count: u32,
    pub constants: ConstantPool,
    pub functions: Vec<BytecodeFunction>,
    pub types: Vec<BytecodeType>,
    pub struct_layouts: Vec<StructLayout>,
    pub choice_layouts: Vec<ChoiceLayout>,
    pub imports: Vec<ImportSlot>,
    pub exports: Vec<ExportSlot>,
    pub init_func_idx: Option<FuncIdx>,
}

impl BytecodeModule {
    /// Converts a validated bytecode type into the ABI contract shared with hosts.
    pub fn boundary_type(
        &self,
        type_index: TypeIdx,
    ) -> Result<galfus_contract::BoundaryType, galfus_contract::BoundaryCodecError> {
        use galfus_contract::{BoundaryCodecError, BoundaryType};

        match self.types.get(type_index.raw() as usize) {
            Some(BytecodeType::Null) => Ok(BoundaryType::Null),
            Some(BytecodeType::Bool) => Ok(BoundaryType::Bool),
            Some(BytecodeType::Int8) => Ok(BoundaryType::I8),
            Some(BytecodeType::Int16) => Ok(BoundaryType::I16),
            Some(BytecodeType::Int32) => Ok(BoundaryType::I32),
            Some(BytecodeType::Int64) => Ok(BoundaryType::I64),
            Some(BytecodeType::Uint8) => Ok(BoundaryType::U8),
            Some(BytecodeType::Uint16) => Ok(BoundaryType::U16),
            Some(BytecodeType::Uint32) => Ok(BoundaryType::U32),
            Some(BytecodeType::Uint64) => Ok(BoundaryType::U64),
            Some(BytecodeType::Float32) => Ok(BoundaryType::F32),
            Some(BytecodeType::Float64) => Ok(BoundaryType::F64),
            Some(BytecodeType::Function { .. }) => Ok(BoundaryType::Function),
            Some(BytecodeType::AdapterHandle(kind)) => {
                Ok(BoundaryType::Handle { kind: kind.clone() })
            }
            Some(BytecodeType::Array(element)) => {
                Ok(BoundaryType::Array(Box::new(self.boundary_type(*element)?)))
            }
            Some(BytecodeType::Nullable(inner)) => Ok(BoundaryType::Nullable(Box::new(
                self.boundary_type(*inner)?,
            ))),
            Some(BytecodeType::Tuple(elements)) => elements
                .iter()
                .copied()
                .map(|element| self.boundary_type(element))
                .collect::<Result<Vec<_>, _>>()
                .map(BoundaryType::Tuple),
            Some(BytecodeType::Choice(_)) | None => Err(BoundaryCodecError::UnsupportedType),
            _ => Err(BoundaryCodecError::UnsupportedType),
        }
    }
}
