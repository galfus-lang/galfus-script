mod export;
#[cfg(test)]
mod tests;

use crate::{
    ImportedMemberKey, ImportedStructFieldDefault, ImportedStructFieldSurface,
    ImportedSurfaceTypes, ImportedType, ModuleAst, ResolutionLayer, StringTable, SymbolKind,
    SyntaxNodeKind, TypeCheckResult, TypeKind,
    type_validation::{
        ImportedChoiceSurface, ImportedConstraintSurface, ImportedFunctionParameterType,
    },
};
pub use export::*;
use galfus_core::{NodeId, SymbolId, TypeId};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSurface {
    exports: Vec<ModuleSurfaceExport>,
    exports_by_name: HashMap<String, usize>,
}

impl ModuleSurface {
    pub fn new(exports: Vec<ModuleSurfaceExport>) -> Self {
        let exports_by_name = exports
            .iter()
            .enumerate()
            .map(|(index, export)| (export.name().to_string(), index))
            .collect();

        Self {
            exports,
            exports_by_name,
        }
    }

    pub fn exports(&self) -> &[ModuleSurfaceExport] {
        self.exports.as_slice()
    }

    pub fn export(&self, name: &str) -> Option<&ModuleSurfaceExport> {
        self.exports_by_name
            .get(name)
            .and_then(|index| self.exports.get(*index))
    }

    pub fn imported_type_for_export(
        &self,
        local_symbol: SymbolId,
        name: &str,
    ) -> Option<ImportedType> {
        let export = self.export(name)?;

        if export.kind().is_nominal_surface_type() {
            return Some(ImportedType::NamedLocal {
                symbol: local_symbol,
            });
        }

        export.ty().cloned()
    }

    pub fn imported_path_type_for_export(
        &self,
        namespace: SymbolId,
        name: &str,
    ) -> Option<ImportedType> {
        if let Some(export) = self.export(name) {
            if export.kind().is_nominal_surface_type() {
                return Some(ImportedType::SurfacePath {
                    namespace,
                    name: name.to_string(),
                });
            }

            return export.ty().map(|ty| ty.relocate(namespace));
        }

        let (owner_name, member_name) = name.rsplit_once("::")?;
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

    pub fn imported_member_path_type_for_named_export(
        &self,
        local_symbol: SymbolId,
        owner_name: &str,
        member_name: &str,
    ) -> Option<ImportedType> {
        let owner = self.export(owner_name)?;
        let member = owner
            .members()
            .iter()
            .find(|member| member.name() == member_name)?;

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
                    .map(|ty| ImportedFunctionParameterType::new(ty.clone()))
                    .collect();

                Some(ImportedType::Function {
                    parameters,
                    return_type: Box::new(owner_type),
                })
            }

            _ => member.ty().cloned(),
        }
    }

    pub fn imported_member_path_type_for_namespace(
        &self,
        namespace: SymbolId,
        owner_name: &str,
        member_name: &str,
    ) -> Option<ImportedType> {
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

    pub fn imported_constraint_for_export(
        &self,
        name: &str,
        namespace: Option<SymbolId>,
    ) -> Option<ImportedConstraintSurface> {
        let export = self.export(name)?;

        if export.kind() != SymbolKind::Constraint {
            return None;
        }

        Some(export.imported_constraint_surface(namespace))
    }

    pub fn imported_choice_for_export(
        &self,
        name: &str,
        namespace: Option<SymbolId>,
        module_path: &str,
    ) -> Option<ImportedChoiceSurface> {
        let export = self.export(name)?;

        if export.kind() != SymbolKind::Choice {
            return None;
        }

        Some(export.imported_choice_surface(namespace, module_path))
    }
}

pub fn build_module_surface(
    source: &galfus_core::SourceFile,
    graph: &ModuleAst,
    type_result: &TypeCheckResult,
    string_table: &StringTable,
) -> ModuleSurface {
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
                    .and_then(|ty| transport_type(resolution, type_result, string_table, ty))
            };

            let members = surface_members_for_export(
                source,
                graph,
                type_result,
                string_table,
                export.symbol(),
            );
            let generic_parameters = surface_generic_parameters(
                graph,
                export.symbol(),
                export.kind(),
                type_result,
                resolution,
                string_table,
            );

            ModuleSurfaceExport::with_members(
                export.name().to_string(),
                export.kind(),
                ty,
                members,
                generic_parameters,
            )
            .with_satisfied_constraints(surface_satisfied_constraints(
                graph,
                type_result,
                string_table,
                export.symbol(),
            ))
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
        if let Some(ty) = surface.imported_path_type_for_export(namespace, export.name()) {
            imported_types
                .insert_member_type(ImportedMemberKey::new(namespace, "", export.name()), ty);
        }

        for member in export.members() {
            if let Some(ty) = surface.imported_member_path_type_for_namespace(
                namespace,
                export.name(),
                member.name(),
            ) {
                imported_types.insert_member_type(
                    ImportedMemberKey::new(namespace, export.name(), member.name()),
                    ty,
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
    module_path: &str,
) -> ImportedSurfaceTypes {
    let mut imported_types = ImportedSurfaceTypes::new();
    let Some(export) = surface.export(name) else {
        return imported_types;
    };

    if let Some(ty) = surface.imported_type_for_export(local_symbol, name) {
        imported_types.insert_symbol_type(local_symbol, ty);
    }

    if export.kind() == SymbolKind::Constraint {
        imported_types
            .insert_symbol_constraint(local_symbol, export.imported_constraint_surface(None));
    }

    if export.kind() == SymbolKind::Choice {
        imported_types.insert_symbol_choice(
            local_symbol,
            export.imported_choice_surface(None, module_path),
        );
    }
    if export.kind() == SymbolKind::Enum {
        imported_types.insert_symbol_enum_values(local_symbol, export.imported_enum_values());
    }

    for member in export.members() {
        if let Some(ty) = surface.imported_member_path_type_for_named_export(
            local_symbol,
            export.name(),
            member.name(),
        ) {
            imported_types
                .insert_member_type(ImportedMemberKey::new(local_symbol, "", member.name()), ty);
        }
    }

    if export.kind() == SymbolKind::Struct {
        let fields = export
            .members()
            .iter()
            .filter(|member| member.kind() == SymbolKind::StructField)
            .filter_map(|member| {
                member.ty().cloned().map(|ty| {
                    ImportedStructFieldSurface::new(
                        member.name().to_string(),
                        ty,
                        member.has_default(),
                        member.default_value(),
                    )
                })
            })
            .collect();
        imported_types.insert_struct_fields(local_symbol, fields);
        imported_types
            .insert_struct_constraints(local_symbol, export.satisfied_constraints().to_vec());
    }

    for struct_export in surface
        .exports()
        .iter()
        .filter(|candidate| candidate.kind() == SymbolKind::Struct)
    {
        imported_types.insert_struct_constraints_by_name(
            struct_export.name().to_string(),
            struct_export.satisfied_constraints().to_vec(),
        );
    }

    if export.kind() == SymbolKind::Function
        && let Some(struct_name) = imported_function_return_struct_name(export.ty())
        && let Some(struct_export) = surface.export(struct_name)
        && struct_export.kind() == SymbolKind::Struct
    {
        for member in struct_export.members() {
            if let Some(ty) = surface.imported_member_path_type_for_named_export(
                local_symbol,
                struct_name,
                member.name(),
            ) {
                imported_types.insert_member_type(
                    ImportedMemberKey::new(local_symbol, struct_name, member.name()),
                    ty,
                );
            }
        }

        let fields = struct_export
            .members()
            .iter()
            .filter(|member| member.kind() == SymbolKind::StructField)
            .filter_map(|member| {
                member.ty().cloned().map(|ty| {
                    ImportedStructFieldSurface::new(
                        member.name().to_string(),
                        ty,
                        member.has_default(),
                        member.default_value(),
                    )
                })
            })
            .collect();
        imported_types.insert_struct_fields(local_symbol, fields);
    }

    imported_types
}

fn surface_satisfied_constraints(
    graph: &ModuleAst,
    type_result: &TypeCheckResult,
    string_table: &StringTable,
    symbol: SymbolId,
) -> Vec<ImportedType> {
    let Some(resolution) = graph.resolution() else {
        return Vec::new();
    };
    let Some(member_scope) = resolution.member_scope(symbol) else {
        return Vec::new();
    };
    let Some(scope) = resolution.scope(member_scope) else {
        return Vec::new();
    };
    let Some(item) = scope.owner() else {
        return Vec::new();
    };
    let Some(satisfies) = graph
        .syntax()
        .first_child_of_kind(item, SyntaxNodeKind::SatisfiesClause)
    else {
        return Vec::new();
    };
    let Some(satisfies) = graph.syntax().node(satisfies) else {
        return Vec::new();
    };

    satisfies
        .children()
        .iter()
        .filter_map(|constraint| type_result.layer().node_type(*constraint))
        .filter_map(|constraint| transport_type(resolution, type_result, string_table, constraint))
        .collect()
}

fn imported_function_return_struct_name(ty: Option<&ImportedType>) -> Option<&str> {
    let ImportedType::Function { return_type, .. } = ty? else {
        return None;
    };

    match return_type.as_ref() {
        ImportedType::LocalPath { name } | ImportedType::SurfacePath { name, .. } => {
            Some(name.as_str())
        }
        _ => None,
    }
}

fn surface_members_for_export(
    source: &galfus_core::SourceFile,
    graph: &ModuleAst,
    type_result: &TypeCheckResult,
    string_table: &StringTable,
    symbol: SymbolId,
) -> Vec<ModuleSurfaceMember> {
    let Some(resolution) = graph.resolution() else {
        return Vec::new();
    };

    let Some(symbol_data) = resolution.symbol(symbol) else {
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
                        .and_then(|ty| transport_type(resolution, type_result, string_table, ty))?;

                    let name = string_table.resolve(*name).unwrap_or("").to_string();
                    let has_default = member.kind() == SymbolKind::StructField
                        && struct_field_has_default(graph, resolution, *member_symbol);
                    let default_value =
                        struct_field_default_value(source, graph, resolution, *member_symbol);
                    let surface_member = if has_default {
                        ModuleSurfaceMember::with_default(
                            name,
                            member.kind(),
                            Some(ty),
                            default_value,
                        )
                    } else {
                        ModuleSurfaceMember::new(name, member.kind(), Some(ty))
                    };

                    Some((member.declaration(), surface_member))
                }

                SymbolKind::ConstraintFunction => {
                    let ty = type_result
                        .layer()
                        .symbol_type(*member_symbol)
                        .and_then(|ty| transport_type(resolution, type_result, string_table, ty))?;

                    Some((
                        member.declaration(),
                        ModuleSurfaceMember::new(
                            string_table.resolve(*name).unwrap_or("").to_string(),
                            member.kind(),
                            Some(ty),
                        ),
                    ))
                }

                SymbolKind::EnumVariant => Some((
                    member.declaration(),
                    ModuleSurfaceMember::new(
                        string_table.resolve(*name).unwrap_or("").to_string(),
                        member.kind(),
                        None,
                    ),
                )),

                SymbolKind::ChoiceVariant => {
                    let payload_types = choice_payload_types(
                        graph,
                        type_result,
                        string_table,
                        member.declaration(),
                    )?;

                    Some((
                        member.declaration(),
                        ModuleSurfaceMember::with_payload(
                            string_table.resolve(*name).unwrap_or("").to_string(),
                            member.kind(),
                            payload_types,
                        ),
                    ))
                }

                _ => None,
            }
        })
        .collect::<Vec<_>>();

    let owner_name = string_table.resolve(symbol_data.name()).unwrap_or("");
    let anchor_prefix = format!("{owner_name}::");
    members.extend(
        resolution
            .symbols()
            .iter()
            .filter(|member| {
                member.kind() == SymbolKind::Function
                    && resolution.export_for_symbol(member.id()).is_some()
                    && string_table
                        .resolve(member.name())
                        .is_some_and(|name| name.starts_with(anchor_prefix.as_str()))
            })
            .filter_map(|member| {
                let name = string_table.resolve(member.name())?;
                let member_name = name.strip_prefix(anchor_prefix.as_str())?.to_string();
                let ty = type_result
                    .layer()
                    .symbol_type(member.id())
                    .and_then(|ty| transport_type(resolution, type_result, string_table, ty))?;

                Some((
                    member.declaration(),
                    ModuleSurfaceMember::new(member_name, member.kind(), Some(ty)),
                ))
            }),
    );
    members.sort_by_key(|(declaration, _)| declaration.raw());
    if resolution
        .symbol(symbol)
        .is_some_and(|item| item.kind() == SymbolKind::Enum)
    {
        for (declaration, member) in &mut members {
            let value = enum_variant_value(graph, source, resolution, symbol, *declaration);
            *member = ModuleSurfaceMember::enum_variant(member.name().to_string(), value);
        }
    }
    members.into_iter().map(|(_, member)| member).collect()
}

fn node_contains_kind(graph: &ModuleAst, node: NodeId, kind: SyntaxNodeKind) -> bool {
    let Some(node) = graph.syntax().node(node) else {
        return false;
    };

    node.kind() == kind
        || node
            .children()
            .iter()
            .any(|child| node_contains_kind(graph, *child, kind))
}

fn struct_field_has_default(
    graph: &ModuleAst,
    resolution: &ResolutionLayer,
    field_symbol: SymbolId,
) -> bool {
    let Some(root) = graph.syntax().root() else {
        return false;
    };
    let Some(field) = find_struct_field(graph, resolution, root, field_symbol) else {
        return false;
    };

    node_contains_kind(graph, field, SyntaxNodeKind::StructFieldDefault)
}

fn struct_field_default_value(
    source: &galfus_core::SourceFile,
    graph: &ModuleAst,
    resolution: &ResolutionLayer,
    field_symbol: SymbolId,
) -> Option<ImportedStructFieldDefault> {
    let root = graph.syntax().root()?;
    let field = find_struct_field(graph, resolution, root, field_symbol)?;
    let default = graph
        .syntax()
        .first_child_of_kind(field, SyntaxNodeKind::StructFieldDefault)?;
    let expression = graph.syntax().child(default, 0)?;

    match graph.syntax().node(expression)?.kind() {
        SyntaxNodeKind::NullLiteral => Some(ImportedStructFieldDefault::Null),
        SyntaxNodeKind::ArrayLiteral
            if graph
                .syntax()
                .node(expression)
                .is_some_and(|array| array.children().is_empty()) =>
        {
            Some(ImportedStructFieldDefault::EmptyArray)
        }
        SyntaxNodeKind::IntegerLiteral => source
            .slice(graph.syntax().node(expression)?.span())?
            .parse::<i64>()
            .ok()
            .map(ImportedStructFieldDefault::Integer),
        _ => None,
    }
}

fn find_struct_field(
    graph: &ModuleAst,
    resolution: &ResolutionLayer,
    node: NodeId,
    field_symbol: SymbolId,
) -> Option<NodeId> {
    if graph
        .syntax()
        .node(node)
        .is_some_and(|field| field.kind() == SyntaxNodeKind::StructField)
        && graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::Identifier)
            .and_then(|identifier| resolution.declaration_symbol(identifier))
            == Some(field_symbol)
    {
        return Some(node);
    }

    graph
        .syntax()
        .node(node)?
        .children()
        .iter()
        .find_map(|child| find_struct_field(graph, resolution, *child, field_symbol))
}

fn enum_variant_value(
    graph: &ModuleAst,
    source: &galfus_core::SourceFile,
    resolution: &crate::ResolutionLayer,
    enum_symbol: SymbolId,
    variant_declaration: NodeId,
) -> i64 {
    let Some(root) = graph.syntax().root() else {
        return 0;
    };
    let Some(enum_item) = find_enum_item(graph, resolution, root, enum_symbol) else {
        return 0;
    };
    let Some(variants) = graph
        .syntax()
        .first_child_of_kind(enum_item, SyntaxNodeKind::EnumVariantList)
    else {
        return 0;
    };

    let mut value = 0i64;
    for variant in graph
        .syntax()
        .node(variants)
        .into_iter()
        .flat_map(|node| node.children())
    {
        if let Some(explicit) = graph
            .syntax()
            .first_child_of_kind(*variant, SyntaxNodeKind::EnumDiscriminant)
            .and_then(|discriminant| graph.syntax().child(discriminant, 0))
            .and_then(|expression| source.slice(graph.syntax().node(expression)?.span()))
            .and_then(|text| text.parse::<i64>().ok())
        {
            value = explicit;
        }
        if graph
            .syntax()
            .first_child_of_kind(*variant, SyntaxNodeKind::Identifier)
            .and_then(|identifier| resolution.declaration_symbol(identifier))
            == resolution.declaration_symbol(variant_declaration)
        {
            return value;
        }
        value += 1;
    }
    0
}

fn find_enum_item(
    graph: &ModuleAst,
    resolution: &crate::ResolutionLayer,
    node: NodeId,
    enum_symbol: SymbolId,
) -> Option<NodeId> {
    if graph
        .syntax()
        .node(node)
        .is_some_and(|item| item.kind() == SyntaxNodeKind::EnumItem)
        && graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::Identifier)
            .and_then(|identifier| resolution.declaration_symbol(identifier))
            == Some(enum_symbol)
    {
        return Some(node);
    }
    graph
        .syntax()
        .node(node)?
        .children()
        .iter()
        .find_map(|child| find_enum_item(graph, resolution, *child, enum_symbol))
}

fn surface_generic_parameters(
    graph: &ModuleAst,
    symbol: SymbolId,
    kind: SymbolKind,
    type_result: &TypeCheckResult,
    resolution: &ResolutionLayer,
    string_table: &StringTable,
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
            transport_type(resolution, type_result, string_table, ty)
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
    string_table: &StringTable,
    declaration: NodeId,
) -> Option<Vec<ImportedType>> {
    let root = graph.syntax().root()?;
    let variant = find_parent_choice_variant(graph, root, declaration)?;
    let Some(payload) = find_descendant_of_kind(graph, variant, SyntaxNodeKind::ChoicePayload)
    else {
        return Some(Vec::new());
    };
    let payload_node = graph.syntax().node(payload)?;

    let resolution = graph.resolution()?;

    payload_node
        .children()
        .iter()
        .map(|child| {
            let type_node = first_type_child(graph, *child).unwrap_or(*child);
            let ty = type_result.layer().node_type(type_node)?;

            transport_type(resolution, type_result, string_table, ty)
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
    string_table: &StringTable,
    ty: TypeId,
) -> Option<ImportedType> {
    match result.layer().table().kind(ty).cloned()? {
        TypeKind::Primitive(primitive) => Some(ImportedType::Primitive(primitive)),

        TypeKind::Array { element } => Some(ImportedType::Array {
            element: Box::new(transport_type(resolution, result, string_table, element)?),
        }),

        TypeKind::Range { element } => Some(ImportedType::Range {
            element: Box::new(transport_type(resolution, result, string_table, element)?),
        }),

        TypeKind::Tuple { elements } => {
            let elements = elements
                .into_iter()
                .map(|element| transport_type(resolution, result, string_table, element))
                .collect::<Option<Vec<_>>>()?;

            Some(ImportedType::Tuple { elements })
        }

        TypeKind::Union { members } => {
            let members = members
                .into_iter()
                .map(|member| transport_type(resolution, result, string_table, member))
                .collect::<Option<Vec<_>>>()?;

            Some(ImportedType::Union { members })
        }

        TypeKind::Function(function) => {
            let parameters = function
                .parameters()
                .iter()
                .map(|parameter| {
                    let ty = transport_type(resolution, result, string_table, parameter.ty())?;

                    if parameter.is_rest() {
                        return Some(ImportedFunctionParameterType::rest(ty));
                    }

                    if parameter.has_default() {
                        return Some(ImportedFunctionParameterType::with_default(
                            ty,
                            parameter.default_value().map(|s| s.to_string()),
                        ));
                    }

                    Some(ImportedFunctionParameterType::new(ty))
                })
                .collect::<Option<Vec<_>>>()?;

            let return_type = Box::new(transport_type(
                resolution,
                result,
                string_table,
                function.return_type(),
            )?);

            Some(ImportedType::Function {
                parameters,
                return_type,
            })
        }
        TypeKind::Named { symbol } => {
            let symbol_data = resolution.symbol(symbol)?;
            if matches!(
                symbol_data.kind(),
                SymbolKind::TypeAlias | SymbolKind::ImportBinding
            ) && let Some(target_ty) = result.layer().symbol_type(symbol)
                && target_ty != ty
            {
                return transport_type(resolution, result, string_table, target_ty);
            }
            let name = string_table
                .resolve(symbol_data.name())
                .unwrap_or("")
                .to_string();
            Some(ImportedType::LocalPath { name })
        }
        TypeKind::Path { root, segments } => {
            if root == SymbolId::new(0)
                && let Some(name) = segments.first()
                && let Some(name_id) = string_table.get(name)
                && let Some(symbol) = resolution.lookup_symbol(resolution.module_scope(), name_id)
                && resolution
                    .symbol(symbol)
                    .is_some_and(|symbol| symbol.kind() == SymbolKind::ImportBinding)
                && let Some(target_ty) = result.layer().symbol_type(symbol)
                && target_ty != ty
            {
                return transport_type(resolution, result, string_table, target_ty);
            }

            if resolution
                .symbol(root)
                .is_some_and(|symbol| symbol.kind() == SymbolKind::ImportBinding)
                && let Some(target_ty) = result.layer().symbol_type(root)
                && target_ty != ty
            {
                return transport_type(resolution, result, string_table, target_ty);
            }

            let symbol_data = resolution.symbol(root)?;
            let mut name = string_table
                .resolve(symbol_data.name())
                .unwrap_or("")
                .to_string();
            for segment in segments {
                name.push_str("::");
                name.push_str(&segment);
            }
            Some(ImportedType::LocalPath { name })
        }
        TypeKind::GenericParameter { symbol } => Some(ImportedType::GenericParameter { symbol }),
        TypeKind::GenericInstance { base, arguments } => {
            let base = Box::new(transport_type(resolution, result, string_table, base)?);
            let arguments = arguments
                .into_iter()
                .map(|arg| transport_type(resolution, result, string_table, arg))
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
