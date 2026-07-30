// =========================================================================
// Constant Pool
// =========================================================================

pub mod graph;
pub mod graph_resolver;
pub mod instruction;
pub mod validation;

pub use graph::{
    BytecodeGraph, BytecodeGraphTransaction, BytecodeGraphTransactionError,
    BytecodeGraphValidationError, BytecodeNode, ImportEdge,
};
pub use graph_resolver::{GraphResolutionError, ModuleImports, ResolvedImport};
pub use instruction::*;
pub use validation::*;

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
    ExternalHandle(String),
    Struct(StructLayoutIdx),
    Array(TypeIdx),
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
pub struct BytecodeFunction {
    pub name: String,
    pub param_count: u8,
    pub local_count: u16,
    pub temp_count: u16,
    pub return_ty: TypeIdx,
    pub instructions: Vec<Instruction>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BytecodeModule {
    pub name: String,
    pub constants: ConstantPool,
    pub functions: Vec<BytecodeFunction>,
    pub types: Vec<BytecodeType>,
    pub struct_layouts: Vec<StructLayout>,
    pub choice_layouts: Vec<ChoiceLayout>,
    pub imports: Vec<ImportSlot>,
    pub exports: Vec<ExportSlot>,
    pub init_func_idx: Option<FuncIdx>,
}
