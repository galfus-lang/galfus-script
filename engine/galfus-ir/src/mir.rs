use galfus_core::{FunctionId, SymbolId, TypeId};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalId(u32);

impl LocalId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(u32);

impl BlockId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn raw(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalDecl {
    pub name: String,
    pub ty: TypeId,
}

#[derive(Debug, Clone)]
pub struct MirModule {
    pub functions: Vec<MirFunction>,
    pub globals: Vec<GlobalDecl>,
    pub constant_pool: Vec<Constant>,
}

#[derive(Debug, Clone)]
pub struct LocalDecl {
    pub id: LocalId,
    pub ty: TypeId,
    pub is_owned: bool,
}

#[derive(Debug, Clone)]
pub struct MirFunction {
    pub id: FunctionId,
    pub name: String,
    pub return_type: TypeId,
    pub parameter_types: Vec<TypeId>,
    pub locals: Vec<LocalDecl>,
    pub blocks: Vec<BasicBlock>,
    pub type_substitutions: HashMap<SymbolId, TypeId>,
    pub is_async: bool,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub parameters: Vec<LocalDecl>,
    pub instructions: Vec<(Instruction, Option<galfus_core::Span>)>,
    pub terminator: (Terminator, Option<galfus_core::Span>),
}

#[derive(Debug, Clone)]
pub enum Instruction {
    Assign(LocalId, RValue),
    Drop(LocalId),
    StoreGlobal(String, Operand),

    /// Store a value into an indexed aggregate:
    ///
    /// `array[index] = value`
    ///
    /// This is a statement-level side effect, not an RValue, because it does
    /// not produce a value. It lowers directly to bytecode `StoreIndex`.
    StoreIndex {
        arr: Operand,
        idx: Operand,
        val: Operand,
    },
    StoreField {
        obj: Operand,
        field_name: String,
        val: Operand,
    },
    Call {
        func: FunctionId,
        args: Vec<Operand>,
        destination: LocalId,
        is_external: bool,
    },
    IndirectCall {
        func: Operand,
        args: Vec<Operand>,
        destination: LocalId,
    },
    ConstraintCall {
        method_name: String,
        obj: Operand,
        args: Vec<Operand>,
        destination: LocalId,
    },
    Await {
        future: Operand,
        destination: LocalId,
        drop_future: bool,
    },
    AwaitAll {
        futures: Vec<Operand>,
        destination: LocalId,
    },
    AwaitRace {
        futures: Vec<Operand>,
        destination: LocalId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirUnaryOp {
    Negate,
    Not,
    BitwiseNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirBinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Power,
    ShiftLeft,
    ShiftRight,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    LogicalAnd,
    LogicalOr,
    NullFallback,
}

#[derive(Debug, Clone)]
pub enum RValue {
    Use(Operand),
    UnaryOp(MirUnaryOp, Operand),
    BinaryOp(MirBinaryOp, Operand, Operand),
    Cast(Operand, TypeId),
    Copy(Operand),
    NewStruct {
        struct_type: TypeId,
        fields: Vec<Operand>,
    },
    NewArray(TypeId, Vec<Operand>),
    NewArrayDynamic(TypeId, Vec<ArrayLiteralElement>),
    /// Zero-initialised allocation with compile-time length.
    /// `array_type` is the full `Array` TypeId.
    /// `element_type` is the element TypeId (for zero-value generation).
    /// `size` is the known compile-time length.
    NewArrayZeroed {
        array_type: TypeId,
        element_type: TypeId,
        size: usize,
    },
    /// Zero-initialised allocation with runtime length.
    NewArrayZeroedDynamic {
        array_type: TypeId,
        element_type: TypeId,
        length: Operand,
    },
    NewTuple(TypeId, Vec<Operand>),
    MemberAccess(Operand, String),
    ArrayIndex(Operand, Operand),
    Choice(TypeId, String, Option<Operand>),
    ChoiceVariantIs(Operand, SymbolId),
    ImportedChoiceVariantIs(Operand, String, String),
    Instanceof(Operand, TypeId),
    LoadGlobal(String),
    Len(Operand),
    CreateFuture {
        func: FunctionId,
        args: Vec<Operand>,
        is_external: bool,
    },
    CreateIndirectFuture {
        func: Operand,
        args: Vec<Operand>,
    },
}

#[derive(Debug, Clone)]
pub enum ArrayLiteralElement {
    Single(Operand),
    Spread(Operand),
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Return(Option<Operand>),
    Jump {
        target: BlockId,
        args: Vec<Operand>,
    },
    Branch {
        cond: Operand,
        true_block: BlockId,
        true_args: Vec<Operand>,
        false_block: BlockId,
        false_args: Vec<Operand>,
    },
    TailCall {
        func: FunctionId,
        args: Vec<Operand>,
        is_external: bool,
    },
    Panic(String),
}

#[derive(Debug, Clone)]
pub enum Operand {
    Constant(Constant),
    ConstRef(usize),
    Local(LocalId),
}

#[derive(Debug, Clone)]
pub enum Constant {
    Null,
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
    Function(FunctionId),
}
