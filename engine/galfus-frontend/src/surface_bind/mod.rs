mod export;
#[cfg(test)]
mod tests;

use crate::{
    AsNameId, ImportedChoiceSurface, ImportedConstraintSurface, ImportedFunctionParameterType,
    ImportedMemberKey, ImportedSurfaceTypes, ImportedType, ModuleAst, NameId, ResolutionLayer,
    SymbolKind, SyntaxNodeKind, TypeCheckResult, TypeKind,
};
pub use export::*;
use galfus_core::{NodeId, SymbolId, TypeId};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSurface {
    exports: Vec<ModuleSurfaceExport>,
    exports_by_name: HashMap<NameId, usize>,
}

impl ModuleSurface {
    pub fn new(exports: Vec<ModuleSurfaceExport>) -> Self {
        let exports_by_name = exports
            .iter()
            .enumerate()
            .map(|(index, export)| (NameId::intern(export.name()), index))
            .collect();

        Self {
            exports,
            exports_by_name,
        }
    }

    pub fn exports(&self) -> &[ModuleSurfaceExport] {
        self.exports.as_slice()
    }

    pub fn export<N: AsNameId>(&self, name: N) -> Option<&ModuleSurfaceExport> {
        self.exports_by_name
            .get(&name.to_name_id())
            .and_then(|index| self.exports.get(*index))
    }

    pub fn imported_type_for_export<N: AsNameId>(
        &self,
        local_symbol: SymbolId,
        name: N,
    ) -> Option<ImportedType> {
        let name_id = name.to_name_id();
        let export = self.export(name_id)?;

        if export.kind().is_nominal_surface_type() {
            return Some(ImportedType::NamedLocal {
                symbol: local_symbol,
            });
        }

        export.ty().map(|ty| ty.relocate(local_symbol))
    }

    pub fn imported_path_type_for_export<N: AsNameId>(
        &self,
        namespace: SymbolId,
        name: N,
    ) -> Option<ImportedType> {
        let name_id = name.to_name_id();
        if let Some(export) = self.export(name_id) {
            if export.kind().is_nominal_surface_type() {
                return Some(ImportedType::SurfacePath {
                    namespace,
                    name: name_id.to_string(),
                });
            }

            return export.ty().map(|ty| ty.relocate(namespace));
        }

        let name_str = name_id.as_str();
        let (owner_name, member_name) = name_str.rsplit_once("::")?;
        let owner = self.export(owner_name)?;
        let member = owner
            .members()
            .iter()
            .find(|member| member.name() == member_name)?;

        match member.kind() {
            SymbolKind::EnumVariant => Some(ImportedType::SurfacePath {
                namespace,
                name: owner_name.to_string(),
            }),

            SymbolKind::ChoiceVariant => {
                let owner_type = ImportedType::SurfacePath {
                    namespace,
                    name: owner_name.to_string(),
                };

                if member.payload_types().is_empty() {
                    return Some(owner_type);
                }

                let parameters = member
                    .payload_types()
                    .iter()
                    .map(|ty| ImportedFunctionParameterType::new(ty.relocate(namespace)))
                    .collect();

                Some(ImportedType::Function {
                    parameters,
                    return_type: Box::new(owner_type),
                })
            }

            _ => member.ty().map(|ty| ty.relocate(namespace)),
        }
    }

    pub fn imported_member_path_type_for_named_export<N1: AsNameId, N2: AsNameId>(
        &self,
        local_symbol: SymbolId,
        owner_name: N1,
        member_name: N2,
    ) -> Option<ImportedType> {
        let owner = self.export(owner_name)?;
        let member_name_id = member_name.to_name_id();
        let member = owner
            .members()
            .iter()
            .find(|member| member.name() == member_name_id.as_str())?;

        match member.kind() {
            SymbolKind::EnumVariant => Some(ImportedType::NamedLocal {
                symbol: local_symbol,
            }),

            SymbolKind::ChoiceVariant => {
                let owner_type = ImportedType::NamedLocal {
                    symbol: local_symbol,
                };

                if member.payload_types().is_empty() {
                    return Some(owner_type);
                }

                let parameters = member
                    .payload_types()
                    .iter()
                    .map(|ty| ImportedFunctionParameterType::new(ty.relocate(local_symbol)))
                    .collect();

                Some(ImportedType::Function {
                    parameters,
                    return_type: Box::new(owner_type),
                })
            }

            _ => member.ty().map(|ty| ty.relocate(local_symbol)),
        }
    }

    pub fn imported_constraint_for_export<N: AsNameId>(
        &self,
        name: N,
    ) -> Option<ImportedConstraintSurface> {
        let name_id = name.to_name_id();
        let export = self.export(name_id)?;

        if export.kind() != SymbolKind::Constraint {
            return None;
        }

        Some(export.imported_constraint_surface())
    }

    pub fn imported_choice_for_export<N: AsNameId>(
        &self,
        name: N,
    ) -> Option<ImportedChoiceSurface> {
        let name_id = name.to_name_id();
        let export = self.export(name_id)?;

        if export.kind() != SymbolKind::Choice {
            return None;
        }

        Some(export.imported_choice_surface())
    }
}

pub fn build_module_surface(graph: &ModuleAst, type_result: &TypeCheckResult) -> ModuleSurface {
    let Some(resolution) = graph.resolution() else {
        return ModuleSurface::new(Vec::new());
    };

    let exports = resolution
        .exports()
        .iter()
        .map(|export| {
            let ty = if export.kind().is_nominal_surface_type() {
                Some(ImportedType::NamedLocal {
                    symbol: export.symbol(),
                })
            } else {
                type_result
                    .layer()
                    .symbol_type(export.symbol())
                    .and_then(|ty| transport_type(resolution, type_result, ty))
            };

            let members = surface_members_for_export(graph, type_result, export.symbol());
            let generic_parameters = surface_generic_parameters(
                graph,
                export.symbol(),
                export.kind(),
                type_result,
                resolution,
            );

            ModuleSurfaceExport::with_members(
                export.name().to_string(),
                export.kind(),
                ty,
                members,
                generic_parameters,
            )
        })
        .collect();

    ModuleSurface::new(exports)
}

pub fn imported_surface_types_for_namespace(
    surface: &ModuleSurface,
    namespace: SymbolId,
) -> ImportedSurfaceTypes {
    let mut imported_types = ImportedSurfaceTypes::new();

    for export in surface.exports() {
        if let Some(ty) = export.ty() {
            imported_types.insert_member_type(
                ImportedMemberKey::new(namespace, "", export.name()),
                ty.clone(),
            );
        }

        for member in export.members() {
            if let Some(ty) = member.ty() {
                imported_types.insert_member_type(
                    ImportedMemberKey::new(namespace, export.name(), member.name()),
                    ty.clone(),
                );
            }
        }
    }

    imported_types
}

pub fn imported_surface_types_for_named_export(
    surface: &ModuleSurface,
    local_symbol: SymbolId,
    name: &str,
) -> ImportedSurfaceTypes {
    let mut imported_types = ImportedSurfaceTypes::new();
    let Some(export) = surface.export(name) else {
        return imported_types;
    };

    if let Some(ty) = export.ty() {
        imported_types.insert_symbol_type(local_symbol, ty.clone());
    }

    if export.kind() == SymbolKind::Constraint {
        imported_types.insert_symbol_constraint(local_symbol, export.imported_constraint_surface());
    }

    if export.kind() == SymbolKind::Choice {
        imported_types.insert_symbol_choice(local_symbol, export.imported_choice_surface());
    }

    for member in export.members() {
        if let Some(ty) = member.ty() {
            imported_types.insert_member_type(
                ImportedMemberKey::new(local_symbol, "", member.name()),
                ty.clone(),
            );
        }
    }

    imported_types
}

fn surface_members_for_export(
    graph: &ModuleAst,
    type_result: &TypeCheckResult,
    symbol: SymbolId,
) -> Vec<ModuleSurfaceMember> {
    let Some(resolution) = graph.resolution() else {
        return Vec::new();
    };

    let Some(member_scope) = resolution.member_scope(symbol) else {
        return Vec::new();
    };

    let Some(scope) = resolution.scope(member_scope) else {
        return Vec::new();
    };

    let mut members = scope
        .symbols()
        .iter()
        .filter_map(|(name, member_symbol)| {
            let member = resolution.symbol(*member_symbol)?;
            match member.kind() {
                SymbolKind::StructField | SymbolKind::ConstraintField => {
                    let ty = type_result
                        .layer()
                        .symbol_type(*member_symbol)
                        .and_then(|ty| transport_type(resolution, type_result, ty))?;

                    Some((
                        member.declaration(),
                        ModuleSurfaceMember::new(name.to_string(), member.kind(), Some(ty)),
                    ))
                }

                SymbolKind::ConstraintFunction => {
                    let ty = type_result
                        .layer()
                        .symbol_type(*member_symbol)
                        .and_then(|ty| transport_type(resolution, type_result, ty))?;

                    Some((
                        member.declaration(),
                        ModuleSurfaceMember::new(name.to_string(), member.kind(), Some(ty)),
                    ))
                }

                SymbolKind::EnumVariant => Some((
                    member.declaration(),
                    ModuleSurfaceMember::new(name.to_string(), member.kind(), None),
                )),

                SymbolKind::ChoiceVariant => {
                    let payload_types =
                        choice_payload_types(graph, type_result, member.declaration())?;

                    Some((
                        member.declaration(),
                        ModuleSurfaceMember::with_payload(
                            name.to_string(),
                            member.kind(),
                            payload_types,
                        ),
                    ))
                }

                _ => None,
            }
        })
        .collect::<Vec<_>>();
    members.sort_by_key(|(declaration, _)| declaration.raw());
    members.into_iter().map(|(_, member)| member).collect()
}

fn surface_generic_parameters(
    graph: &ModuleAst,
    symbol: SymbolId,
    kind: SymbolKind,
    type_result: &TypeCheckResult,
    resolution: &ResolutionLayer,
) -> Vec<ImportedType> {
    match kind {
        SymbolKind::Constraint | SymbolKind::Choice | SymbolKind::Struct | SymbolKind::Function => {
        }
        _ => return Vec::new(),
    }

    let Some(member_scope) = resolution.member_scope(symbol) else {
        return Vec::new();
    };

    let Some(scope) = resolution.scope(member_scope) else {
        return Vec::new();
    };

    let Some(owner) = scope.owner() else {
        return Vec::new();
    };

    let local_parameters = declaration_generic_parameters_in_node(graph, owner);

    local_parameters
        .into_iter()
        .filter_map(|param_symbol| {
            let ty = type_result.layer().symbol_type(param_symbol)?;
            transport_type(resolution, type_result, ty)
        })
        .collect()
}

fn declaration_generic_parameters_in_node(graph: &ModuleAst, node: NodeId) -> Vec<SymbolId> {
    let mut symbols = Vec::new();
    collect_generic_parameters_in_node(graph, node, &mut symbols);
    symbols
}

fn collect_generic_parameters_in_node(
    graph: &ModuleAst,
    node: NodeId,
    symbols: &mut Vec<SymbolId>,
) {
    let Some(syntax_node) = graph.syntax().node(node) else {
        return;
    };

    if let Some(symbol) = graph
        .resolution()
        .and_then(|resolution| resolution.declaration_symbol(node))
        && let Some(sym) = graph.resolution().and_then(|res| res.symbol(symbol))
        && sym.kind() == SymbolKind::GenericParameter
    {
        symbols.push(symbol);
    }

    for child in syntax_node.children() {
        collect_generic_parameters_in_node(graph, *child, symbols);
    }
}

fn choice_payload_types(
    graph: &ModuleAst,
    type_result: &TypeCheckResult,
    declaration: NodeId,
) -> Option<Vec<ImportedType>> {
    let root = graph.syntax().root()?;
    let variant = find_parent_choice_variant(graph, root, declaration)?;
    let payload = find_descendant_of_kind(graph, variant, SyntaxNodeKind::ChoicePayload)?;
    let payload_node = graph.syntax().node(payload)?;

    let resolution = graph.resolution()?;

    payload_node
        .children()
        .iter()
        .map(|child| {
            let type_node = first_type_child(graph, *child).unwrap_or(*child);
            let ty = type_result.layer().node_type(type_node)?;

            transport_type(resolution, type_result, ty)
        })
        .collect()
}

fn find_parent_choice_variant(
    graph: &ModuleAst,
    node: NodeId,
    declaration: NodeId,
) -> Option<NodeId> {
    let syntax_node = graph.syntax().node(node)?;

    if syntax_node.kind() == SyntaxNodeKind::ChoiceVariant
        && graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::Identifier)
            == Some(declaration)
    {
        return Some(node);
    }

    for child in syntax_node.children() {
        if let Some(found) = find_parent_choice_variant(graph, *child, declaration) {
            return Some(found);
        }
    }

    None
}

fn find_descendant_of_kind(
    graph: &ModuleAst,
    node: NodeId,
    kind: SyntaxNodeKind,
) -> Option<NodeId> {
    let syntax_node = graph.syntax().node(node)?;

    for child in syntax_node.children() {
        let child_node = graph.syntax().node(*child)?;

        if child_node.kind() == kind {
            return Some(*child);
        }

        if let Some(found) = find_descendant_of_kind(graph, *child, kind) {
            return Some(found);
        }
    }

    None
}

fn first_type_child(graph: &ModuleAst, node: NodeId) -> Option<NodeId> {
    let syntax_node = graph.syntax().node(node)?;

    if syntax_node.kind().is_type() {
        return Some(node);
    }

    syntax_node.children().iter().copied().find(|child| {
        graph
            .syntax()
            .node(*child)
            .is_some_and(|node| node.kind().is_type())
    })
}

fn transport_type(
    resolution: &ResolutionLayer,
    result: &TypeCheckResult,
    ty: TypeId,
) -> Option<ImportedType> {
    match result.layer().table().kind(ty).cloned()? {
        TypeKind::Primitive(primitive) => Some(ImportedType::Primitive(primitive)),

        TypeKind::Array { element } => Some(ImportedType::Array {
            element: Box::new(transport_type(resolution, result, element)?),
        }),

        TypeKind::Range { element } => Some(ImportedType::Range {
            element: Box::new(transport_type(resolution, result, element)?),
        }),

        TypeKind::Tuple { elements } => {
            let elements = elements
                .into_iter()
                .map(|element| transport_type(resolution, result, element))
                .collect::<Option<Vec<_>>>()?;

            Some(ImportedType::Tuple { elements })
        }

        TypeKind::Union { members } => {
            let members = members
                .into_iter()
                .map(|member| transport_type(resolution, result, member))
                .collect::<Option<Vec<_>>>()?;

            Some(ImportedType::Union { members })
        }

        TypeKind::Function(function) => {
            let parameters = function
                .parameters()
                .iter()
                .map(|parameter| {
                    let ty = transport_type(resolution, result, parameter.ty())?;

                    if parameter.is_rest() {
                        return Some(ImportedFunctionParameterType::rest(ty));
                    }

                    if parameter.has_default() {
                        return Some(ImportedFunctionParameterType::with_default(ty));
                    }

                    Some(ImportedFunctionParameterType::new(ty))
                })
                .collect::<Option<Vec<_>>>()?;

            let return_type = Box::new(transport_type(resolution, result, function.return_type())?);

            Some(ImportedType::Function {
                parameters,
                return_type,
            })
        }
        TypeKind::Named { symbol } => {
            let symbol_data = resolution.symbol(symbol)?;
            if symbol_data.kind() == SymbolKind::TypeAlias
                && let Some(target_ty) = result.layer().symbol_type(symbol)
            {
                return transport_type(resolution, result, target_ty);
            }
            let name = symbol_data.name().to_string();
            Some(ImportedType::LocalPath { name })
        }
        TypeKind::Path { root, segments } => {
            let symbol_data = resolution.symbol(root)?;
            let mut name = symbol_data.name().to_string();
            for segment in segments {
                name.push_str("::");
                name.push_str(&segment);
            }
            Some(ImportedType::LocalPath { name })
        }
        TypeKind::GenericParameter { symbol } => Some(ImportedType::GenericParameter { symbol }),
        TypeKind::GenericInstance { base, arguments } => {
            let base = Box::new(transport_type(resolution, result, base)?);
            let arguments = arguments
                .into_iter()
                .map(|arg| transport_type(resolution, result, arg))
                .collect::<Option<Vec<_>>>()?;
            Some(ImportedType::GenericInstance { base, arguments })
        }
        _ => None,
    }
}

impl SymbolKind {
    pub fn is_type_definition(self) -> bool {
        matches!(
            self,
            Self::Struct | Self::Enum | Self::Choice | Self::Constraint | Self::TypeAlias
        )
    }

    pub fn is_nominal_surface_type(self) -> bool {
        matches!(
            self,
            Self::Struct | Self::Enum | Self::Choice | Self::Constraint
        )
    }
}
