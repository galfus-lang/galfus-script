mod ownership_model;

use crate::{PrimitiveType, TypeLayer};
use galfus_core::{DiagnosticBag, NodeId, SymbolId, TypeId};
pub use ownership_model::*;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedFunctionParameterType {
    ty: ImportedType,
    is_rest: bool,
    has_default: bool,
}

impl ImportedFunctionParameterType {
    pub fn new(ty: ImportedType) -> Self {
        Self {
            ty,
            is_rest: false,
            has_default: false,
        }
    }

    pub fn rest(ty: ImportedType) -> Self {
        Self {
            ty,
            is_rest: true,
            has_default: false,
        }
    }

    pub fn with_default(ty: ImportedType) -> Self {
        Self {
            ty,
            is_rest: false,
            has_default: true,
        }
    }

    pub fn ty(&self) -> &ImportedType {
        &self.ty
    }

    pub fn is_rest(&self) -> bool {
        self.is_rest
    }

    pub fn has_default(&self) -> bool {
        self.has_default
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportedType {
    Primitive(PrimitiveType),
    NamedLocal {
        symbol: SymbolId,
    },
    SurfacePath {
        namespace: SymbolId,
        name: String,
    },
    Array {
        element: Box<ImportedType>,
    },
    Range {
        element: Box<ImportedType>,
    },
    Tuple {
        elements: Vec<ImportedType>,
    },
    Union {
        members: Vec<ImportedType>,
    },
    Function {
        parameters: Vec<ImportedFunctionParameterType>,
        return_type: Box<ImportedType>,
    },
    LocalPath {
        name: String,
    },
    GenericParameter {
        symbol: SymbolId,
    },
    GenericInstance {
        base: Box<ImportedType>,
        arguments: Vec<ImportedType>,
    },
}

impl ImportedType {
    pub fn relocate(&self, namespace: SymbolId) -> Self {
        match self {
            Self::Primitive(primitive) => Self::Primitive(*primitive),
            Self::NamedLocal { symbol } => Self::NamedLocal { symbol: *symbol },
            Self::SurfacePath {
                namespace: ns,
                name,
            } => Self::SurfacePath {
                namespace: *ns,
                name: name.clone(),
            },
            Self::LocalPath { name } => Self::SurfacePath {
                namespace,
                name: name.clone(),
            },
            Self::Array { element } => Self::Array {
                element: Box::new(element.relocate(namespace)),
            },
            Self::Range { element } => Self::Range {
                element: Box::new(element.relocate(namespace)),
            },
            Self::Tuple { elements } => Self::Tuple {
                elements: elements.iter().map(|e| e.relocate(namespace)).collect(),
            },
            Self::Union { members } => Self::Union {
                members: members.iter().map(|m| m.relocate(namespace)).collect(),
            },
            Self::Function {
                parameters,
                return_type,
            } => Self::Function {
                parameters: parameters
                    .iter()
                    .map(|p| ImportedFunctionParameterType {
                        ty: p.ty.relocate(namespace),
                        is_rest: p.is_rest,
                        has_default: p.has_default,
                    })
                    .collect(),
                return_type: Box::new(return_type.relocate(namespace)),
            },
            Self::GenericParameter { symbol } => Self::GenericParameter { symbol: *symbol },
            Self::GenericInstance { base, arguments } => Self::GenericInstance {
                base: Box::new(base.relocate(namespace)),
                arguments: arguments.iter().map(|a| a.relocate(namespace)).collect(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedConstraintMember {
    name: String,
    ty: ImportedType,
}

impl ImportedConstraintMember {
    pub fn new(name: String, ty: ImportedType) -> Self {
        Self { name, ty }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn ty(&self) -> &ImportedType {
        &self.ty
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedConstraintSurface {
    name: String,
    generic_parameters: Vec<ImportedType>,
    fields: Vec<ImportedConstraintMember>,
    functions: Vec<ImportedConstraintMember>,
}

impl ImportedConstraintSurface {
    pub fn new(
        name: String,
        generic_parameters: Vec<ImportedType>,
        fields: Vec<ImportedConstraintMember>,
        functions: Vec<ImportedConstraintMember>,
    ) -> Self {
        Self {
            name,
            generic_parameters,
            fields,
            functions,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn generic_parameter_count(&self) -> usize {
        self.generic_parameters.len()
    }

    pub fn generic_parameters(&self) -> &[ImportedType] {
        self.generic_parameters.as_slice()
    }

    pub fn fields(&self) -> &[ImportedConstraintMember] {
        self.fields.as_slice()
    }

    pub fn functions(&self) -> &[ImportedConstraintMember] {
        self.functions.as_slice()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedChoiceVariant {
    name: String,
    payload_types: Vec<ImportedType>,
}

impl ImportedChoiceVariant {
    pub fn new(name: String, payload_types: Vec<ImportedType>) -> Self {
        Self {
            name,
            payload_types,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn payload_types(&self) -> &[ImportedType] {
        self.payload_types.as_slice()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedChoiceSurface {
    name: String,
    variants: Vec<ImportedChoiceVariant>,
    generic_parameters: Vec<ImportedType>,
}

impl ImportedChoiceSurface {
    pub fn new(
        name: String,
        variants: Vec<ImportedChoiceVariant>,
        generic_parameters: Vec<ImportedType>,
    ) -> Self {
        Self {
            name,
            variants,
            generic_parameters,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn variants(&self) -> &[ImportedChoiceVariant] {
        self.variants.as_slice()
    }

    pub fn generic_parameters(&self) -> &[ImportedType] {
        self.generic_parameters.as_slice()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportedMemberKey {
    namespace: SymbolId,
    owner: String,
    member: String,
}

impl ImportedMemberKey {
    pub fn new(namespace: SymbolId, owner: impl Into<String>, member: impl Into<String>) -> Self {
        Self {
            namespace,
            owner: owner.into(),
            member: member.into(),
        }
    }

    pub fn namespace(&self) -> SymbolId {
        self.namespace
    }

    pub fn owner(&self) -> &str {
        self.owner.as_str()
    }

    pub fn member(&self) -> &str {
        self.member.as_str()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportedSurfaceTypes {
    symbol_types: HashMap<SymbolId, ImportedType>,
    path_types: HashMap<NodeId, ImportedType>,
    member_types: HashMap<ImportedMemberKey, ImportedType>,
    symbol_constraints: HashMap<SymbolId, ImportedConstraintSurface>,
    path_constraints: HashMap<NodeId, ImportedConstraintSurface>,
    symbol_choices: HashMap<SymbolId, ImportedChoiceSurface>,
    path_choices: HashMap<NodeId, ImportedChoiceSurface>,
}

impl ImportedSurfaceTypes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn symbol_types(&self) -> &HashMap<SymbolId, ImportedType> {
        &self.symbol_types
    }

    pub fn path_types(&self) -> &HashMap<NodeId, ImportedType> {
        &self.path_types
    }

    pub fn member_types(&self) -> &HashMap<ImportedMemberKey, ImportedType> {
        &self.member_types
    }

    pub fn symbol_constraints(&self) -> &HashMap<SymbolId, ImportedConstraintSurface> {
        &self.symbol_constraints
    }

    pub fn path_constraints(&self) -> &HashMap<NodeId, ImportedConstraintSurface> {
        &self.path_constraints
    }

    pub fn symbol_choices(&self) -> &HashMap<SymbolId, ImportedChoiceSurface> {
        &self.symbol_choices
    }

    pub fn path_choices(&self) -> &HashMap<NodeId, ImportedChoiceSurface> {
        &self.path_choices
    }

    pub fn insert_symbol_type(&mut self, symbol: SymbolId, ty: ImportedType) {
        self.symbol_types.insert(symbol, ty);
    }

    pub fn insert_path_type(&mut self, node: NodeId, ty: ImportedType) {
        self.path_types.insert(node, ty);
    }

    pub fn insert_member_type(&mut self, key: ImportedMemberKey, ty: ImportedType) {
        self.member_types.insert(key, ty);
    }

    pub fn insert_symbol_constraint(
        &mut self,
        symbol: SymbolId,
        constraint: ImportedConstraintSurface,
    ) {
        self.symbol_constraints.insert(symbol, constraint);
    }

    pub fn insert_path_constraint(&mut self, node: NodeId, constraint: ImportedConstraintSurface) {
        self.path_constraints.insert(node, constraint);
    }

    pub fn insert_symbol_choice(&mut self, symbol: SymbolId, choice: ImportedChoiceSurface) {
        self.symbol_choices.insert(symbol, choice);
    }

    pub fn insert_path_choice(&mut self, node: NodeId, choice: ImportedChoiceSurface) {
        self.path_choices.insert(node, choice);
    }

    pub fn extend(&mut self, other: ImportedSurfaceTypes) {
        self.symbol_types.extend(other.symbol_types);
        self.path_types.extend(other.path_types);
        self.member_types.extend(other.member_types);
        self.symbol_constraints.extend(other.symbol_constraints);
        self.path_constraints.extend(other.path_constraints);
        self.symbol_choices.extend(other.symbol_choices);
        self.path_choices.extend(other.path_choices);
    }
}

#[derive(Debug, Clone)]
pub struct LoweredImportedChoice {
    pub name: String,
    pub variants: Vec<LoweredImportedChoiceVariant>,
    pub generic_parameters: Vec<SymbolId>,
}

#[derive(Debug, Clone)]
pub struct LoweredImportedChoiceVariant {
    pub name: String,
    pub payload_types: Vec<TypeId>,
}

#[derive(Debug, Clone)]
pub struct TypeCheckResult {
    pub(super) layer: TypeLayer,
    pub(super) diagnostics: DiagnosticBag,
    pub(super) ownership_metadata: OwnershipMetadata,
    pub imported_symbol_choices: HashMap<SymbolId, LoweredImportedChoice>,
    pub imported_path_choices: HashMap<NodeId, LoweredImportedChoice>,
    pub(super) range_desugars: HashMap<NodeId, RangeDesugarTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeDesugarTarget {
    Exclusive,
    Stepped,
}

impl TypeCheckResult {
    pub fn new(layer: TypeLayer, diagnostics: DiagnosticBag) -> Self {
        Self::with_ownership_metadata(
            layer,
            diagnostics,
            OwnershipMetadata::default(),
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
        )
    }

    pub(super) fn with_ownership_metadata(
        layer: TypeLayer,
        diagnostics: DiagnosticBag,
        ownership_metadata: OwnershipMetadata,
        imported_symbol_choices: HashMap<SymbolId, LoweredImportedChoice>,
        imported_path_choices: HashMap<NodeId, LoweredImportedChoice>,
        range_desugars: HashMap<NodeId, RangeDesugarTarget>,
    ) -> Self {
        Self {
            layer,
            diagnostics,
            ownership_metadata,
            imported_symbol_choices,
            imported_path_choices,
            range_desugars,
        }
    }

    pub fn layer(&self) -> &TypeLayer {
        &self.layer
    }

    pub fn layer_mut(&mut self) -> &mut TypeLayer {
        &mut self.layer
    }

    pub fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    pub fn ownership_metadata(&self) -> &OwnershipMetadata {
        &self.ownership_metadata
    }

    pub fn range_desugar(&self, node: NodeId) -> Option<RangeDesugarTarget> {
        self.range_desugars.get(&node).copied()
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.has_errors()
    }

    pub fn into_layer(self) -> TypeLayer {
        self.layer
    }
}
