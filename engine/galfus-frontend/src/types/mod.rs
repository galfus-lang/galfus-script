#[cfg(test)]
mod tests;

use galfus_core::{NodeId, SymbolId, TypeId};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveType {
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
    Float16,
    Float32,
    Float64,
}

impl PrimitiveType {
    pub const ALL: [Self; 13] = [
        Self::Null,
        Self::Bool,
        Self::Int8,
        Self::Int16,
        Self::Int32,
        Self::Int64,
        Self::Uint8,
        Self::Uint16,
        Self::Uint32,
        Self::Uint64,
        Self::Float16,
        Self::Float32,
        Self::Float64,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool => "bool",
            Self::Int8 => "i8",
            Self::Int16 => "i16",
            Self::Int32 => "i32",
            Self::Int64 => "i64",
            Self::Uint8 => "u8",
            Self::Uint16 => "u16",
            Self::Uint32 => "u32",
            Self::Uint64 => "u64",
            Self::Float16 => "f16",
            Self::Float32 => "f32",
            Self::Float64 => "f64",
        }
    }

    pub fn is_int(self) -> bool {
        matches!(self, Self::Int8 | Self::Int16 | Self::Int32 | Self::Int64)
    }

    pub fn is_uint(self) -> bool {
        matches!(
            self,
            Self::Uint8 | Self::Uint16 | Self::Uint32 | Self::Uint64
        )
    }

    pub fn is_float(self) -> bool {
        matches!(self, Self::Float16 | Self::Float32 | Self::Float64)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionParameterType {
    ty: TypeId,
    is_rest: bool,
    has_default: bool,
}

impl FunctionParameterType {
    pub fn new(ty: TypeId) -> Self {
        Self {
            ty,
            is_rest: false,
            has_default: false,
        }
    }

    pub fn rest(ty: TypeId) -> Self {
        Self {
            ty,
            is_rest: true,
            has_default: false,
        }
    }

    pub fn with_default(ty: TypeId) -> Self {
        Self {
            ty,
            is_rest: false,
            has_default: true,
        }
    }

    pub fn ty(&self) -> TypeId {
        self.ty
    }

    pub fn is_rest(&self) -> bool {
        self.is_rest
    }

    pub fn has_default(&self) -> bool {
        self.has_default
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionType {
    parameters: Vec<FunctionParameterType>,
    return_type: TypeId,
}

impl FunctionType {
    pub fn new(parameters: Vec<FunctionParameterType>, return_type: TypeId) -> Self {
        Self {
            parameters,
            return_type,
        }
    }

    pub fn parameters(&self) -> &[FunctionParameterType] {
        self.parameters.as_slice()
    }

    pub fn return_type(&self) -> TypeId {
        self.return_type
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeKind {
    Primitive(PrimitiveType),

    Named {
        symbol: SymbolId,
    },

    Path {
        root: SymbolId,
        segments: Vec<String>,
    },

    GenericParameter {
        symbol: SymbolId,
    },

    Array {
        element: TypeId,
    },

    Range {
        element: TypeId,
    },

    Tuple {
        elements: Vec<TypeId>,
    },

    Union {
        members: Vec<TypeId>,
    },

    Function(FunctionType),

    GenericInstance {
        base: TypeId,
        arguments: Vec<TypeId>,
    },

    Error,
}

#[derive(Debug, Clone)]
pub struct TypeTable {
    types: Vec<TypeKind>,
    types_by_kind: HashMap<TypeKind, TypeId>,
    primitive_types: HashMap<PrimitiveType, TypeId>,
}

impl TypeTable {
    pub fn new() -> Self {
        let mut table = Self {
            types: Vec::new(),
            types_by_kind: HashMap::new(),
            primitive_types: HashMap::new(),
        };

        for primitive in PrimitiveType::ALL {
            let id = table.intern(TypeKind::Primitive(primitive));
            table.primitive_types.insert(primitive, id);
        }

        table
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    pub fn kind(&self, id: TypeId) -> Option<&TypeKind> {
        self.types.get(id.raw() as usize)
    }

    pub fn primitive(&self, primitive: PrimitiveType) -> TypeId {
        *self
            .primitive_types
            .get(&primitive)
            .expect("primitive type must be pre-interned")
    }

    pub fn primitive_family(&mut self, name: &str) -> Option<TypeId> {
        let members = match name {
            "int" => PrimitiveType::ALL
                .into_iter()
                .filter(|primitive| primitive.is_int())
                .map(|primitive| self.primitive(primitive))
                .collect::<Vec<_>>(),
            "uint" => PrimitiveType::ALL
                .into_iter()
                .filter(|primitive| primitive.is_uint())
                .map(|primitive| self.primitive(primitive))
                .collect::<Vec<_>>(),
            "float" => PrimitiveType::ALL
                .into_iter()
                .filter(|primitive| primitive.is_float())
                .map(|primitive| self.primitive(primitive))
                .collect::<Vec<_>>(),
            _ => return None,
        };

        Some(self.intern_union(members))
    }

    pub fn intern(&mut self, kind: TypeKind) -> TypeId {
        if let Some(existing) = self.types_by_kind.get(&kind).copied() {
            return existing;
        }

        let id = TypeId::new(self.types.len() as u32);

        self.types.push(kind.clone());
        self.types_by_kind.insert(kind, id);

        id
    }

    pub fn intern_named(&mut self, symbol: SymbolId) -> TypeId {
        self.intern(TypeKind::Named { symbol })
    }

    pub fn intern_generic_parameter(&mut self, symbol: SymbolId) -> TypeId {
        self.intern(TypeKind::GenericParameter { symbol })
    }

    pub fn intern_array(&mut self, element: TypeId) -> TypeId {
        self.intern(TypeKind::Array { element })
    }

    pub fn intern_range(&mut self, element: TypeId) -> TypeId {
        self.intern(TypeKind::Range { element })
    }

    pub fn intern_tuple(&mut self, elements: Vec<TypeId>) -> TypeId {
        self.intern(TypeKind::Tuple { elements })
    }

    pub fn intern_function(
        &mut self,
        parameters: Vec<FunctionParameterType>,
        return_type: TypeId,
    ) -> TypeId {
        self.intern(TypeKind::Function(FunctionType::new(
            parameters,
            return_type,
        )))
    }

    pub fn intern_generic_instance(&mut self, base: TypeId, arguments: Vec<TypeId>) -> TypeId {
        self.intern(TypeKind::GenericInstance { base, arguments })
    }

    pub fn intern_union<I>(&mut self, members: I) -> TypeId
    where
        I: IntoIterator<Item = TypeId>,
    {
        let mut normalized = Vec::new();

        for member in members {
            match self.kind(member).cloned() {
                Some(TypeKind::Union { members }) => {
                    normalized.extend(members);
                }
                _ => normalized.push(member),
            }
        }

        normalized.sort_by_key(|ty| ty.raw());
        normalized.dedup();

        if normalized.len() == 1 {
            return normalized[0];
        }

        self.intern(TypeKind::Union {
            members: normalized,
        })
    }

    pub fn intern_path(&mut self, root: SymbolId, segments: Vec<String>) -> TypeId {
        self.intern(TypeKind::Path { root, segments })
    }

    pub fn error(&mut self) -> TypeId {
        self.intern(TypeKind::Error)
    }

    pub fn describe(&self, id: TypeId) -> String {
        let Some(kind) = self.kind(id) else {
            return "<missing type>".to_string();
        };

        match kind {
            TypeKind::Primitive(primitive) => primitive.name().to_string(),

            TypeKind::Named { symbol } => {
                format!("symbol#{}", symbol.raw())
            }

            TypeKind::Path { root, segments } => {
                let path = segments.join("::");
                format!("symbol#{}::{path}", root.raw())
            }

            TypeKind::GenericParameter { symbol } => {
                format!("generic#{}", symbol.raw())
            }

            TypeKind::Array { element } => {
                format!("[{}]", self.describe(*element))
            }

            TypeKind::Range { element } => {
                format!("range<{}>", self.describe(*element))
            }

            TypeKind::Tuple { elements } => {
                let elements = elements
                    .iter()
                    .map(|element| self.describe(*element))
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("({elements})")
            }

            TypeKind::Union { members } => members
                .iter()
                .map(|member| self.describe(*member))
                .collect::<Vec<_>>()
                .join(" | "),

            TypeKind::Function(function) => {
                let parameters = function
                    .parameters()
                    .iter()
                    .map(|parameter| {
                        let mut text = self.describe(parameter.ty());

                        if parameter.is_rest() {
                            text = format!("...{text}");
                        }

                        if parameter.has_default() {
                            text = format!("{text} =");
                        }

                        text
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                format!(
                    "fn({parameters}): {}",
                    self.describe(function.return_type())
                )
            }

            TypeKind::GenericInstance { base, arguments } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.describe(*argument))
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("{}<{arguments}>", self.describe(*base))
            }

            TypeKind::Error => "<error>".to_string(),
        }
    }
}

impl Default for TypeTable {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Default)]
pub struct TypeLayer {
    table: TypeTable,
    node_types: HashMap<NodeId, TypeId>,
    symbol_types: HashMap<SymbolId, TypeId>,
}

impl TypeLayer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn table(&self) -> &TypeTable {
        &self.table
    }

    pub fn table_mut(&mut self) -> &mut TypeTable {
        &mut self.table
    }

    pub fn bind_node_type(&mut self, node: NodeId, ty: TypeId) {
        self.node_types.insert(node, ty);
    }

    pub fn node_type(&self, node: NodeId) -> Option<TypeId> {
        self.node_types.get(&node).copied()
    }

    pub fn bind_symbol_type(&mut self, symbol: SymbolId, ty: TypeId) {
        self.symbol_types.insert(symbol, ty);
    }

    pub fn symbol_type(&self, symbol: SymbolId) -> Option<TypeId> {
        self.symbol_types.get(&symbol).copied()
    }

    pub fn node_types(&self) -> &HashMap<NodeId, TypeId> {
        &self.node_types
    }

    pub fn symbol_types(&self) -> &HashMap<SymbolId, TypeId> {
        &self.symbol_types
    }
}
