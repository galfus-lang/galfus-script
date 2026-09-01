// =========================================================================
// Operand Indices (Newtype Wrappers)
// =========================================================================

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Reg(pub u16);

impl Reg {
    pub const fn raw(&self) -> u16 {
        self.0
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ConstIdx(pub u16);

impl ConstIdx {
    pub const fn raw(&self) -> u16 {
        self.0
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct TypeIdx(pub u16);

impl TypeIdx {
    pub const fn raw(&self) -> u16 {
        self.0
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct FuncIdx(pub u16);

impl FuncIdx {
    pub const fn raw(&self) -> u16 {
        self.0
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct GlobalIdx(pub u16);

impl GlobalIdx {
    pub const fn raw(&self) -> u16 {
        self.0
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct FieldIdx(pub u16);

impl FieldIdx {
    pub const fn raw(&self) -> u16 {
        self.0
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct StructLayoutIdx(pub u16);

impl StructLayoutIdx {
    pub const fn raw(&self) -> u16 {
        self.0
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ChoiceLayoutIdx(pub u16);

impl ChoiceLayoutIdx {
    pub const fn raw(&self) -> u16 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ImmediateBinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    ShiftLeft,
    ShiftRight,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ImmediateValue {
    I32(i32),
    I64(i64),
    U32(u32),
    U64(u64),
    F32(u32),
    F64(u64),
}

// =========================================================================
// Opcode Instruction Set
// =========================================================================

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Instruction {
    // Category A: Data Movement & Constants
    LoadConst {
        dest: Reg,
        const_idx: ConstIdx,
    },
    Move {
        dest: Reg,
        src: Reg,
    },
    LoadGlobal {
        dest: Reg,
        module_id: galfus_core::ModuleId,
        global_idx: GlobalIdx,
    },
    StoreGlobal {
        module_id: galfus_core::ModuleId,
        global_idx: GlobalIdx,
        src: Reg,
    },
    LoadNull {
        dest: Reg,
    },

    // Category B: Unary & Binary Operations
    Add {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Sub {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Mul {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Div {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Rem {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Pow {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Neg {
        dest: Reg,
        src: Reg,
    },
    Not {
        dest: Reg,
        src: Reg,
    },
    BitNot {
        dest: Reg,
        src: Reg,
    },
    Shl {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Shr {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    And {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Or {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Xor {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Eq {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Ne {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Lt {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Le {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Gt {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    Ge {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },

    // --- AOT Specialized I32 Operations ---
    AddI32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    SubI32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    MulI32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    DivI32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    RemI32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    EqI32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    NeI32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    LtI32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    LeI32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    GtI32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    GeI32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },

    // --- AOT Specialized I64 Operations ---
    AddI64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    SubI64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    MulI64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    DivI64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    RemI64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    EqI64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    NeI64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    LtI64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    LeI64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    GtI64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    GeI64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },

    // --- AOT Specialized F32 Operations ---
    AddF32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    SubF32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    MulF32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    DivF32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    RemF32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    EqF32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    NeF32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    LtF32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    LeF32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    GtF32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    GeF32 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },

    // --- AOT Specialized F64 Operations ---
    AddF64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    SubF64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    MulF64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    DivF64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    RemF64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    EqF64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    NeF64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    LtF64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    LeF64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    GtF64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    GeF64 {
        dest: Reg,
        lhs: Reg,
        rhs: Reg,
    },
    /// Typed binary operation with an exact-width immediate right operand.
    BinaryImmediate {
        dest: Reg,
        lhs: Reg,
        operation: ImmediateBinaryOp,
        rhs: ImmediateValue,
    },
    Fallback {
        dest: Reg,
        src: Reg,
        fallback: Reg,
    },

    // Category C: Control Flow & Subroutines
    Jump {
        offset: i32,
    },
    JumpTrue {
        cond: Reg,
        offset: i32,
    },
    JumpFalse {
        cond: Reg,
        offset: i32,
    },
    JumpNull {
        val: Reg,
        offset: i32,
    },
    Call {
        dest: Reg,
        func: FuncIdx,
        args_start: Reg,
        arg_count: u8,
    },
    TailCall {
        func: FuncIdx,
        args_start: Reg,
        arg_count: u8,
    },
    /// Dynamic method call resolved at runtime by name. Looks up a function
    /// whose name matches `name_const` (a string constant) and calls it with
    /// `obj` as the first argument followed by `arg_count - 1` extra args
    /// starting at `args_start`. The `dest` is written by the callee's `Ret`.
    CallMethod {
        dest: Reg,
        obj: Reg,
        name_const: ConstIdx,
        args_start: Reg,
        arg_count: u8,
        arg_types: Box<[TypeIdx]>,
        return_type: Option<TypeIdx>,
    },
    CallDynamic {
        dest: Reg,
        func_reg: Reg,
        args_start: Reg,
        arg_count: u8,
    },
    Ret {
        src: Reg,
    },
    RetNull,
    Panic {
        const_idx: ConstIdx,
    },

    // Category D: Heaps, Structs & Collections
    AllocLocal {
        dest: Reg,
        type_idx: TypeIdx,
    },
    LoadField {
        dest: Reg,
        obj: Reg,
        field: FieldIdx,
    },
    StoreField {
        obj: Reg,
        field: FieldIdx,
        val: Reg,
    },
    NewArray {
        dest: Reg,
        type_idx: TypeIdx,
        len_reg: Reg,
    },
    LoadIndex {
        dest: Reg,
        arr: Reg,
        idx: Reg,
    },
    StoreIndex {
        arr: Reg,
        idx: Reg,
        val: Reg,
    },
    NewTuple {
        dest: Reg,
        type_idx: TypeIdx,
        start: Reg,
        count: u8,
    },
    NewChoice {
        dest: Reg,
        type_idx: TypeIdx,
        variant_idx: u16,
        payload: Reg,
    },
    Cast {
        dest: Reg,
        src: Reg,
        type_idx: TypeIdx,
    },
    Copy {
        dest: Reg,
        src: Reg,
    },
    Instanceof {
        dest: Reg,
        src: Reg,
        type_idx: TypeIdx,
    },

    // Category E: Memory Ownership
    Drop {
        reg: Reg,
    },

    // Category G: Future Activations
    AwaitFuture {
        dest: Reg,
        future_id: Reg,
        return_type: TypeIdx,
    },
    CreateFuture {
        dest: Reg,
        func: FuncIdx,
        args_start: Reg,
        arg_count: u8,
        arg_types: Box<[TypeIdx]>,
        return_type: TypeIdx,
    },
    /// Creates and immediately awaits a Future without materializing its handle.
    CreateAwaitFuture {
        dest: Reg,
        operation: Box<str>,
        args_start: Reg,
        arg_count: u8,
        arg_types: Box<[TypeIdx]>,
        return_type: TypeIdx,
    },
    /// Calls an internal synchronous operation without materializing a Future.
    CallInternalThread {
        dest: Reg,
        operation: Box<str>,
        args_start: Reg,
        arg_count: u8,
        arg_types: Box<[TypeIdx]>,
        return_type: TypeIdx,
    },
    CreateIndirectFuture {
        dest: Reg,
        func_reg: Reg,
        args_start: Reg,
        arg_count: u8,
        arg_types: Box<[TypeIdx]>,
        return_type: TypeIdx,
    },
    AwaitAll {
        dest: Reg,
        futures_start: Reg,
        count: u8,
        return_type: TypeIdx,
    },
    AwaitRace {
        dest: Reg,
        futures_start: Reg,
        count: u8,
        return_type: TypeIdx,
    },
    Len {
        dest: Reg,
        src: Reg,
    },
    CopyArray {
        dest: Reg,
        dest_start: Reg,
        src: Reg,
    },
    CallInternalMath {
        dest: Reg,
        operation: u8,
        args_start: Reg,
        arg_count: u8,
    },
}
