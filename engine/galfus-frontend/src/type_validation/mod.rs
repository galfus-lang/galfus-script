mod access;
mod assignability;
mod assignments;
mod builtin_constraints;
mod calls;
mod constraints;
mod constraints_application;
mod control_flow;
mod declarations;
mod decorators;
mod diagnostics;
mod diagnostics_extra;
mod enums;
mod expressions;
mod function_expressions;
mod function_stamps;
mod generic_expressions;
mod inferred_structs;
mod initializers;
mod instanceof;
mod literals;
mod matches;
mod model;
mod operators;
mod ownership;
mod ownership_validation;
mod ranges;
mod returns;
mod semantics;
mod structs;
mod support;
#[cfg(test)]
mod tests;
mod typeofs;
mod variants;

use crate::{FunctionParameterType, ModuleAst, SymbolKind, SyntaxNodeKind, TypeLayer, bind_types};
use galfus_core::{DiagnosticBag, NodeId, SourceFile, SymbolId, TypeId};
pub use model::*;
use std::collections::HashMap;

pub const IMPLICIT_FUTURE_SYMBOL: u32 = u32::MAX;

struct DeclarationTypeChecker<'a> {
    source: &'a SourceFile,
    graph: &'a ModuleAst,
    layer: TypeLayer,
    diagnostics: DiagnosticBag,
    ownership_metadata: OwnershipMetadata,
    imported_member_types: HashMap<ImportedMemberKey, TypeId>,
    imported_struct_fields: HashMap<SymbolId, Vec<ImportedStructField>>,
    imported_symbol_constraints: HashMap<SymbolId, LoweredImportedConstraint>,
    imported_path_constraints: HashMap<NodeId, LoweredImportedConstraint>,
    imported_symbol_choices: HashMap<SymbolId, LoweredImportedChoice>,
    imported_path_choices: HashMap<NodeId, LoweredImportedChoice>,
    imported_symbol_enum_values: HashMap<SymbolId, Vec<(String, i64)>>,
    active_type_substitutions: Vec<HashMap<SymbolId, TypeId>>,
    imported_generic_params: HashMap<SymbolId, SymbolId>,
    control_targets: Vec<control_flow::ControlTarget>,

    range_desugars: HashMap<NodeId, RangeDesugarTarget>,
    string_table: &'a crate::StringTable,
    is_provider_module: bool,
}

#[derive(Debug, Clone)]
struct LoweredImportedConstraint {
    name: String,
    generic_parameters: Vec<SymbolId>,
    fields: Vec<LoweredImportedConstraintMember>,
    functions: Vec<LoweredImportedConstraintMember>,
}

#[derive(Debug, Clone)]
struct LoweredImportedConstraintMember {
    name: String,
    ty: TypeId,
}

impl<'a> DeclarationTypeChecker<'a> {
    fn new(
        source: &'a SourceFile,
        graph: &'a ModuleAst,
        layer: TypeLayer,
        string_table: &'a crate::StringTable,
        is_provider_module: bool,
    ) -> Self {
        Self {
            source,
            graph,
            layer,
            diagnostics: DiagnosticBag::new(),
            ownership_metadata: OwnershipMetadata::default(),
            imported_member_types: HashMap::new(),
            imported_struct_fields: HashMap::new(),
            imported_symbol_constraints: HashMap::new(),
            imported_path_constraints: HashMap::new(),
            imported_symbol_choices: HashMap::new(),
            imported_path_choices: HashMap::new(),
            imported_symbol_enum_values: HashMap::new(),
            active_type_substitutions: Vec::new(),
            imported_generic_params: HashMap::new(),
            control_targets: Vec::new(),

            range_desugars: HashMap::new(),
            string_table,
            is_provider_module,
        }
    }

    fn resume(
        source: &'a SourceFile,
        graph: &'a ModuleAst,
        previous_result: TypeCheckResult,
        string_table: &'a crate::StringTable,
        is_provider_module: bool,
    ) -> Self {
        Self {
            source,
            graph,
            layer: previous_result.layer,
            diagnostics: previous_result.diagnostics,
            ownership_metadata: previous_result.ownership_metadata,
            imported_member_types: HashMap::new(),
            imported_struct_fields: previous_result.imported_struct_fields,
            imported_symbol_constraints: HashMap::new(),
            imported_path_constraints: HashMap::new(),
            imported_symbol_choices: previous_result.imported_symbol_choices,
            imported_path_choices: previous_result.imported_path_choices,
            imported_symbol_enum_values: previous_result.imported_symbol_enum_values,
            active_type_substitutions: Vec::new(),
            imported_generic_params: HashMap::new(),
            control_targets: Vec::new(),

            range_desugars: previous_result.range_desugars,
            string_table,
            is_provider_module,
        }
    }

    fn into_result(self) -> TypeCheckResult {
        TypeCheckResult::with_ownership_metadata(
            self.layer,
            self.diagnostics,
            self.ownership_metadata,
            TypeCheckSupplementalData {
                imported_symbol_choices: self.imported_symbol_choices,
                imported_path_choices: self.imported_path_choices,
                imported_struct_fields: self.imported_struct_fields,
                imported_symbol_enum_values: self.imported_symbol_enum_values,
                range_desugars: self.range_desugars,
            },
        )
    }

    pub(super) fn is_async_function(&self, node: NodeId) -> bool {
        let Some(metadata) = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::KeywordMetadataList)
        else {
            return false;
        };

        self.graph
            .syntax()
            .node(metadata)
            .into_iter()
            .flat_map(|metadata| metadata.children())
            .filter_map(|flag| {
                self.graph
                    .syntax()
                    .first_child_of_kind(*flag, SyntaxNodeKind::Identifier)
            })
            .any(|flag| self.node_text(flag) == "async")
    }

    pub(super) fn async_future_type(&mut self, _node: NodeId, payload: TypeId) -> TypeId {
        let future_base = self.graph.resolution().and_then(|resolution| {
            resolution
                .symbols()
                .iter()
                .find(|symbol| self.string_table.resolve(symbol.name()) == Some("Future"))
                .map(|symbol| {
                    self.layer
                        .symbol_type(symbol.id())
                        .unwrap_or_else(|| self.layer.table_mut().intern_named(symbol.id()))
                })
        });

        let future_base = future_base.unwrap_or_else(|| {
            self.layer
                .table_mut()
                .intern_named(SymbolId::new(IMPLICIT_FUTURE_SYMBOL))
        });
        self.layer
            .table_mut()
            .intern_generic_instance(future_base, vec![payload])
    }

    fn check_declarations(&mut self) {
        self.bind_builtin_symbol_types();
        self.bind_builtin_constraint_symbol_types();
        self.bind_named_type_definition_symbols();

        let Some(root) = self.graph.syntax().root() else {
            return;
        };

        self.bind_node_types(root);
    }

    fn check_definitions(&mut self) {
        let Some(root) = self.graph.syntax().root() else {
            return;
        };

        self.check_decorators(root);
        self.check_keyword_metadata(root);
        self.check_control_flow(root);
        self.check_initializer_types(root);
        self.check_expression_statements(root);
        self.check_enum_types(root);
        self.check_return_types(root, None);
        self.check_assignment_types(root);
        self.check_constraint_satisfies(root);
        self.check_generic_parameter_bounds(root);
        self.check_function_stamps(root);
        self.check_semantic_rules(root);
        self.check_ownership_metadata(root);
    }

    fn describe_type_for_diagnostic(&self, ty: TypeId) -> String {
        let resolved = self.resolve_alias_type(ty);

        self.layer.table().describe(resolved)
    }

    fn bind_imported_symbol_types(&mut self, imported_types: &HashMap<SymbolId, ImportedType>) {
        for (symbol, imported_type) in imported_types {
            let ty = self.lower_imported_type(imported_type);
            self.layer.bind_symbol_type(*symbol, ty);
        }
    }

    fn bind_imported_path_types(&mut self, imported_types: &HashMap<NodeId, ImportedType>) {
        for (node, imported_type) in imported_types {
            let ty = self.lower_imported_type(imported_type);
            self.layer.bind_node_type(*node, ty);
        }
    }

    fn bind_imported_member_types(
        &mut self,
        imported_types: &HashMap<ImportedMemberKey, ImportedType>,
    ) {
        for (key, imported_type) in imported_types {
            let ty = self.lower_imported_type(imported_type);
            self.imported_member_types.insert(key.clone(), ty);
        }
    }

    fn bind_imported_struct_fields(
        &mut self,
        imported_fields: &HashMap<SymbolId, Vec<ImportedStructFieldSurface>>,
    ) {
        for (symbol, fields) in imported_fields {
            let fields = fields
                .iter()
                .map(|field| ImportedStructField {
                    name: field.name().to_string(),
                    ty: self.lower_imported_type(field.ty()),
                    has_default: field.has_default(),
                    default_value: field.default_value(),
                })
                .collect();
            self.imported_struct_fields.insert(*symbol, fields);
        }
    }

    fn bind_imported_symbol_constraints(
        &mut self,
        imported_constraints: &HashMap<SymbolId, ImportedConstraintSurface>,
    ) {
        for (symbol, imported_constraint) in imported_constraints {
            let constraint = self.lower_imported_constraint(imported_constraint);
            self.imported_symbol_constraints.insert(*symbol, constraint);
        }
    }

    fn bind_imported_path_constraints(
        &mut self,
        imported_constraints: &HashMap<NodeId, ImportedConstraintSurface>,
    ) {
        for (node, imported_constraint) in imported_constraints {
            let constraint = self.lower_imported_constraint(imported_constraint);
            self.imported_path_constraints.insert(*node, constraint);
        }
    }

    fn lower_imported_constraint(
        &mut self,
        imported_constraint: &ImportedConstraintSurface,
    ) -> LoweredImportedConstraint {
        LoweredImportedConstraint {
            name: imported_constraint.name().to_string(),
            generic_parameters: imported_constraint
                .generic_parameters()
                .iter()
                .filter_map(|parameter| self.lower_imported_generic_parameter(parameter))
                .collect(),
            fields: imported_constraint
                .fields()
                .iter()
                .map(|field| LoweredImportedConstraintMember {
                    name: field.name().to_string(),
                    ty: self.lower_imported_type(field.ty()),
                })
                .collect(),
            functions: imported_constraint
                .functions()
                .iter()
                .map(|function| LoweredImportedConstraintMember {
                    name: function.name().to_string(),
                    ty: self.lower_imported_type(function.ty()),
                })
                .collect(),
        }
    }

    fn bind_imported_symbol_choices(
        &mut self,
        imported_choices: &HashMap<SymbolId, ImportedChoiceSurface>,
    ) {
        for (symbol, imported_choice) in imported_choices {
            let choice = self.lower_imported_choice(imported_choice);
            self.imported_symbol_choices.insert(*symbol, choice);
        }
    }

    fn bind_imported_path_choices(
        &mut self,
        imported_choices: &HashMap<NodeId, ImportedChoiceSurface>,
    ) {
        for (node, imported_choice) in imported_choices {
            let choice = self.lower_imported_choice(imported_choice);
            self.imported_path_choices.insert(*node, choice);
        }
    }

    fn bind_imported_symbol_enum_values(
        &mut self,
        imported_values: &HashMap<SymbolId, Vec<(String, i64)>>,
    ) {
        self.imported_symbol_enum_values
            .extend(imported_values.clone());
    }

    fn lower_imported_choice(
        &mut self,
        imported_choice: &ImportedChoiceSurface,
    ) -> LoweredImportedChoice {
        let generic_parameters = imported_choice
            .generic_parameters()
            .iter()
            .filter_map(|parameter| self.lower_imported_generic_parameter(parameter))
            .collect();

        LoweredImportedChoice {
            name: imported_choice.name().to_string(),
            generic_parameters,
            variants: imported_choice
                .variants()
                .iter()
                .map(|variant| LoweredImportedChoiceVariant {
                    name: variant.name().to_string(),
                    payload_types: variant
                        .payload_types()
                        .iter()
                        .map(|ty| self.lower_imported_type(ty))
                        .collect(),
                })
                .collect(),
        }
    }

    fn lower_imported_generic_parameter(
        &mut self,
        imported_type: &ImportedType,
    ) -> Option<SymbolId> {
        let ImportedType::GenericParameter { symbol } = imported_type else {
            return None;
        };

        let next_id = 1_000_000 + self.imported_generic_params.len() as u32;
        Some(
            *self
                .imported_generic_params
                .entry(*symbol)
                .or_insert_with(|| SymbolId::new(next_id)),
        )
    }

    fn lower_imported_type(&mut self, imported_type: &ImportedType) -> TypeId {
        match imported_type {
            ImportedType::Primitive(primitive) => self.layer.table().primitive(*primitive),

            ImportedType::NamedLocal { symbol } => self.layer.table_mut().intern_named(*symbol),

            ImportedType::SurfacePath { namespace, name } => self
                .layer
                .table_mut()
                .intern_path(*namespace, name.split("::").map(str::to_string).collect()),

            ImportedType::Array { element } => {
                let element = self.lower_imported_type(element);
                self.layer.table_mut().intern_array(element)
            }

            ImportedType::Range { element } => {
                let element = self.lower_imported_type(element);
                self.layer.table_mut().intern_range(element)
            }

            ImportedType::Tuple { elements } => {
                let elements = elements
                    .iter()
                    .map(|element| self.lower_imported_type(element))
                    .collect::<Vec<_>>();

                self.layer.table_mut().intern_tuple(elements)
            }

            ImportedType::Union { members } => {
                let members = members
                    .iter()
                    .map(|member| self.lower_imported_type(member))
                    .collect::<Vec<_>>();

                self.layer.table_mut().intern_union(members)
            }

            ImportedType::Function {
                parameters,
                return_type,
            } => {
                let parameters = parameters
                    .iter()
                    .map(|parameter| {
                        let ty = self.lower_imported_type(parameter.ty());

                        if parameter.is_rest() {
                            return FunctionParameterType::rest(ty);
                        }

                        if parameter.has_default() {
                            return FunctionParameterType::with_default(
                                ty,
                                parameter.default_value().map(|s| s.to_string()),
                            );
                        }

                        FunctionParameterType::new(ty)
                    })
                    .collect::<Vec<_>>();

                let return_type = self.lower_imported_type(return_type);

                self.layer
                    .table_mut()
                    .intern_function(parameters, return_type, false)
            }

            ImportedType::LocalPath { name } => {
                let imported_symbol = self.graph.resolution().and_then(|resolution| {
                    let name = self.string_table.get(name)?;
                    resolution
                        .symbols()
                        .iter()
                        .find(|symbol| {
                            symbol.name() == name && symbol.kind() == SymbolKind::ImportBinding
                        })
                        .map(|symbol| symbol.id())
                });

                if let Some(symbol) = imported_symbol {
                    self.layer.table_mut().intern_named(symbol)
                } else {
                    self.layer.table_mut().intern_path(
                        SymbolId::new(0),
                        name.split("::").map(str::to_string).collect(),
                    )
                }
            }

            ImportedType::GenericParameter { symbol } => {
                let next_id = 1_000_000 + self.imported_generic_params.len() as u32;
                let mapped_symbol = *self
                    .imported_generic_params
                    .entry(*symbol)
                    .or_insert_with(|| SymbolId::new(next_id));
                self.layer
                    .table_mut()
                    .intern_generic_parameter(mapped_symbol)
            }

            ImportedType::GenericInstance { base, arguments } => {
                let base = self.lower_imported_type(base);
                let arguments = arguments
                    .iter()
                    .map(|arg| self.lower_imported_type(arg))
                    .collect::<Vec<_>>();
                self.layer
                    .table_mut()
                    .intern_generic_instance(base, arguments)
            }
        }
    }

    fn check_expression_statements(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if syntax_node.kind() == SyntaxNodeKind::ExpressionStatement
            && let Some(expression) = self.graph.syntax().child(node, 0)
        {
            self.infer_expression_type(expression);
        }

        for child in syntax_node.children() {
            self.check_expression_statements(*child);
        }
    }
}

pub fn check_declaration_types(
    source: &SourceFile,
    graph: &ModuleAst,
    string_table: &crate::StringTable,
    is_provider_module: bool,
) -> TypeCheckResult {
    let lowering = bind_types(source, graph, string_table);

    let mut checker = DeclarationTypeChecker::new(
        source,
        graph,
        lowering.into_layer(),
        string_table,
        is_provider_module,
    );
    checker.check_declarations();
    checker.into_result()
}

pub fn check_definition_types(
    source: &SourceFile,
    graph: &ModuleAst,
    previous_result: TypeCheckResult,
    string_table: &crate::StringTable,
    is_provider_module: bool,
) -> TypeCheckResult {
    let mut checker = DeclarationTypeChecker::resume(
        source,
        graph,
        previous_result,
        string_table,
        is_provider_module,
    );
    checker.check_definitions();
    checker.into_result()
}

pub fn check_definition_types_with_surfaces(
    source: &SourceFile,
    graph: &ModuleAst,
    previous_result: TypeCheckResult,
    imported_types: &ImportedSurfaceTypes,
    string_table: &crate::StringTable,
    is_provider_module: bool,
) -> TypeCheckResult {
    let mut checker = DeclarationTypeChecker::resume(
        source,
        graph,
        previous_result,
        string_table,
        is_provider_module,
    );
    checker.bind_imported_symbol_types(imported_types.symbol_types());
    checker.bind_imported_path_types(imported_types.path_types());
    checker.bind_imported_member_types(imported_types.member_types());
    checker.bind_imported_struct_fields(imported_types.struct_fields());
    checker.bind_imported_symbol_constraints(imported_types.symbol_constraints());
    checker.bind_imported_path_constraints(imported_types.path_constraints());
    checker.bind_imported_symbol_choices(imported_types.symbol_choices());
    checker.bind_imported_path_choices(imported_types.path_choices());
    checker.bind_imported_symbol_enum_values(imported_types.symbol_enum_values());
    checker.check_definitions();
    checker.into_result()
}
