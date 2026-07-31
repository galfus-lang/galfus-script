use crate::{
    ImportedChoiceSurface, ImportedChoiceVariant, ImportedConstraintMember,
    ImportedConstraintSurface, ImportedType, SymbolKind,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSurfaceExport {
    name: String,
    kind: SymbolKind,
    ty: Option<ImportedType>,
    members: Vec<ModuleSurfaceMember>,
    generic_parameters: Vec<ImportedType>,
}

impl ModuleSurfaceExport {
    pub fn new(name: String, kind: SymbolKind, ty: Option<ImportedType>) -> Self {
        Self::with_members(name, kind, ty, Vec::new(), Vec::new())
    }

    pub fn with_members(
        name: String,
        kind: SymbolKind,
        ty: Option<ImportedType>,
        members: Vec<ModuleSurfaceMember>,
        generic_parameters: Vec<ImportedType>,
    ) -> Self {
        Self {
            name,
            kind,
            ty,
            members,
            generic_parameters,
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

    pub(super) fn imported_constraint_surface(&self) -> ImportedConstraintSurface {
        let fields = self
            .members
            .iter()
            .filter_map(|member| {
                if member.kind() != SymbolKind::ConstraintField {
                    return None;
                }

                Some(ImportedConstraintMember::new(
                    member.name().to_string(),
                    member.ty()?.clone(),
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
                    member.ty()?.clone(),
                ))
            })
            .collect();

        ImportedConstraintSurface::new(
            self.name.clone(),
            self.generic_parameters.clone(),
            fields,
            functions,
        )
    }

    pub(super) fn imported_choice_surface(&self) -> ImportedChoiceSurface {
        let variants = self
            .members
            .iter()
            .filter_map(|member| {
                if member.kind() != SymbolKind::ChoiceVariant {
                    return None;
                }

                Some(ImportedChoiceVariant::new(
                    member.name().to_string(),
                    member.payload_types().to_vec(),
                ))
            })
            .collect();

        ImportedChoiceSurface::new(self.name.clone(), variants, self.generic_parameters.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSurfaceMember {
    name: String,
    kind: SymbolKind,
    ty: Option<ImportedType>,
    payload_types: Vec<ImportedType>,
}

impl ModuleSurfaceMember {
    pub fn new(name: String, kind: SymbolKind, ty: Option<ImportedType>) -> Self {
        Self {
            name,
            kind,
            ty,
            payload_types: Vec::new(),
        }
    }

    pub fn with_payload(name: String, kind: SymbolKind, payload_types: Vec<ImportedType>) -> Self {
        Self {
            name,
            kind,
            ty: None,
            payload_types,
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
}
