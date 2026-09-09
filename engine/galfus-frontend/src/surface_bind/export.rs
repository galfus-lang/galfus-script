use crate::{
    ImportedChoiceSurface, ImportedChoiceVariant, ImportedConstraintMember,
    ImportedConstraintSurface, ImportedStructFieldDefault, ImportedType, SymbolKind,
};
use galfus_core::DefId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSurfaceExport {
    name: String,
    pub def_id: DefId,
    kind: SymbolKind,
    ty: Option<ImportedType>,
    members: Vec<ModuleSurfaceMember>,
    generic_parameters: Vec<ImportedType>,
    satisfied_constraints: Vec<ImportedType>,
    choice_def_id: Option<DefId>,
}

impl ModuleSurfaceExport {
    pub fn new(name: String, def_id: DefId, kind: SymbolKind, ty: Option<ImportedType>) -> Self {
        Self::with_members(name, def_id, kind, ty, Vec::new(), Vec::new())
    }

    pub fn with_members(
        name: String,
        def_id: DefId,
        kind: SymbolKind,
        ty: Option<ImportedType>,
        members: Vec<ModuleSurfaceMember>,
        generic_parameters: Vec<ImportedType>,
    ) -> Self {
        Self {
            name,
            def_id,
            kind,
            ty,
            members,
            generic_parameters,
            satisfied_constraints: Vec::new(),
            choice_def_id: None,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn kind(&self) -> SymbolKind {
        self.kind
    }

    pub fn ty(&self) -> Option<&ImportedType> {
        self.ty.as_ref()
    }

    pub fn members(&self) -> &[ModuleSurfaceMember] {
        self.members.as_slice()
    }

    pub fn generic_parameter_count(&self) -> usize {
        self.generic_parameters.len()
    }

    pub fn generic_parameters(&self) -> &[ImportedType] {
        self.generic_parameters.as_slice()
    }

    pub fn satisfied_constraints(&self) -> &[ImportedType] {
        self.satisfied_constraints.as_slice()
    }

    pub(crate) fn with_satisfied_constraints(
        mut self,
        satisfied_constraints: Vec<ImportedType>,
    ) -> Self {
        self.satisfied_constraints = satisfied_constraints;
        self
    }

    pub(crate) fn with_choice_def_id(mut self, def_id: DefId) -> Self {
        self.choice_def_id = Some(def_id);
        self
    }

    pub(crate) fn has_choice_surface(&self) -> bool {
        self.kind == SymbolKind::Choice || self.choice_def_id.is_some()
    }

    pub(super) fn imported_constraint_surface(
        &self,
        namespace: Option<galfus_core::SymbolId>,
    ) -> ImportedConstraintSurface {
        let fields = self
            .members
            .iter()
            .filter_map(|member| {
                if member.kind() != SymbolKind::ConstraintField {
                    return None;
                }

                Some(ImportedConstraintMember::new(
                    member.name().to_string(),
                    if let Some(ns) = namespace {
                        member.ty()?.relocate(ns)
                    } else {
                        member.ty()?.clone()
                    },
                ))
            })
            .collect();

        let functions = self
            .members
            .iter()
            .filter_map(|member| {
                if member.kind() != SymbolKind::ConstraintFunction {
                    return None;
                }

                Some(ImportedConstraintMember::new(
                    member.name().to_string(),
                    if let Some(ns) = namespace {
                        member.ty()?.relocate(ns)
                    } else {
                        member.ty()?.clone()
                    },
                ))
            })
            .collect();

        ImportedConstraintSurface::new(
            self.name.clone(),
            self.def_id,
            self.generic_parameters
                .iter()
                .map(|p| {
                    if let Some(ns) = namespace {
                        p.relocate(ns)
                    } else {
                        p.clone()
                    }
                })
                .collect(),
            fields,
            functions,
        )
    }

    pub(super) fn imported_choice_surface(
        &self,
        namespace: Option<galfus_core::SymbolId>,
    ) -> ImportedChoiceSurface {
        let variants = self
            .members
            .iter()
            .filter_map(|member| {
                if member.kind() != SymbolKind::ChoiceVariant {
                    return None;
                }

                Some(ImportedChoiceVariant::new(
                    member.name().to_string(),
                    member
                        .payload_types()
                        .iter()
                        .map(|ty| {
                            if let Some(ns) = namespace {
                                ty.relocate(ns)
                            } else {
                                ty.clone()
                            }
                        })
                        .collect(),
                ))
            })
            .collect();

        ImportedChoiceSurface::new(
            self.name.clone(),
            self.choice_def_id.unwrap_or(self.def_id),
            variants,
            self.generic_parameters
                .iter()
                .map(|p| {
                    if let Some(ns) = namespace {
                        p.relocate(ns)
                    } else {
                        p.clone()
                    }
                })
                .collect(),
        )
    }

    pub(super) fn imported_enum_values(&self) -> Vec<(String, i64)> {
        self.members
            .iter()
            .filter(|member| member.kind() == SymbolKind::EnumVariant)
            .map(|member| (member.name.clone(), member.enum_value.unwrap_or_default()))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSurfaceMember {
    name: String,
    kind: SymbolKind,
    ty: Option<ImportedType>,
    payload_types: Vec<ImportedType>,
    enum_value: Option<i64>,
    has_default: bool,
    default_value: Option<ImportedStructFieldDefault>,
}

impl ModuleSurfaceMember {
    pub fn new(name: String, kind: SymbolKind, ty: Option<ImportedType>) -> Self {
        Self {
            name,
            kind,
            ty,
            payload_types: Vec::new(),
            enum_value: None,
            has_default: false,
            default_value: None,
        }
    }

    pub fn with_default(
        name: String,
        kind: SymbolKind,
        ty: Option<ImportedType>,
        default_value: Option<ImportedStructFieldDefault>,
    ) -> Self {
        Self {
            name,
            kind,
            ty,
            payload_types: Vec::new(),
            enum_value: None,
            has_default: true,
            default_value,
        }
    }

    pub fn enum_variant(name: String, value: i64) -> Self {
        Self {
            name,
            kind: SymbolKind::EnumVariant,
            ty: None,
            payload_types: Vec::new(),
            enum_value: Some(value),
            has_default: false,
            default_value: None,
        }
    }

    pub fn with_payload(name: String, kind: SymbolKind, payload_types: Vec<ImportedType>) -> Self {
        Self {
            name,
            kind,
            ty: None,
            payload_types,
            enum_value: None,
            has_default: false,
            default_value: None,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn kind(&self) -> SymbolKind {
        self.kind
    }

    pub fn ty(&self) -> Option<&ImportedType> {
        self.ty.as_ref()
    }

    pub fn payload_types(&self) -> &[ImportedType] {
        self.payload_types.as_slice()
    }

    pub fn has_default(&self) -> bool {
        self.has_default
    }

    pub fn default_value(&self) -> Option<ImportedStructFieldDefault> {
        self.default_value
    }
}
