use super::{DeclarationTypeChecker, LoweredImportedConstraint};
use crate::{SymbolKind, SyntaxNodeKind};
use galfus_core::{NodeId, SymbolId, TypeId};
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct ConstraintFieldInfo {
    name: String,
    ty: TypeId,
}

#[derive(Debug, Clone)]
struct StructFieldInfo {
    node: NodeId,
    name: String,
    ty: TypeId,
}

#[derive(Debug, Clone)]
struct ConstraintFunctionInfo {
    name: String,
    ty: TypeId,
}

#[derive(Debug, Clone)]
struct StructFunctionInfo {
    node: NodeId,
    name: String,
    ty: TypeId,
}

pub(super) type TypeSubstitution = HashMap<SymbolId, TypeId>;

#[derive(Debug, Clone)]
pub(super) struct ConstraintApplication {
    pub(super) symbol: SymbolId,
    pub(super) constraint_name: String,
    pub(super) substitution: TypeSubstitution,
    pub(super) imported_constraint: Option<LoweredImportedConstraint>,
}

#[derive(Debug, Clone)]
pub(super) enum ConstraintApplicationError {
    InvalidTarget,
    GenericArgumentCountMismatch {
        constraint_name: String,
        expected: usize,
        actual: usize,
    },
}

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn check_constraint_satisfies(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        match syntax_node.kind() {
            SyntaxNodeKind::StructItem => {
                self.check_struct_satisfies(node);
            }
            SyntaxNodeKind::GenericParameterConstraint => {
                self.check_generic_parameter_constraint(node);
            }
            _ => {}
        }

        let children = syntax_node.children().to_vec();

        for child in children {
            self.check_constraint_satisfies(child);
        }
    }

    fn check_struct_satisfies(&mut self, struct_item: NodeId) {
        let Some(satisfies) = self
            .graph
            .syntax()
            .first_child_of_kind(struct_item, SyntaxNodeKind::SatisfiesClause)
        else {
            return;
        };

        let Some((struct_symbol, struct_name)) = self.struct_item_symbol(struct_item) else {
            return;
        };

        let struct_fields = self.struct_satisfies_fields(struct_symbol);

        let struct_functions = self.struct_satisfies_functions(struct_name.as_str());

        let constraints = self
            .graph
            .syntax()
            .node(satisfies)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();

        for constraint_type in constraints {
            self.check_single_satisfies_constraint(
                struct_item,
                constraint_type,
                struct_name.as_str(),
                &struct_fields,
                &struct_functions,
            );
        }
    }

    fn check_single_satisfies_constraint(
        &mut self,
        struct_item: NodeId,
        constraint_type: NodeId,
        struct_name: &str,
        struct_fields: &[StructFieldInfo],
        struct_functions: &[StructFunctionInfo],
    ) {
        let target_name = self.node_text(constraint_type);

        let application = match self.constraint_application(constraint_type) {
            Ok(application) => application,

            Err(ConstraintApplicationError::InvalidTarget) => {
                self.report_invalid_satisfies_target(constraint_type, target_name.as_str());
                return;
            }

            Err(ConstraintApplicationError::GenericArgumentCountMismatch {
                constraint_name,
                expected,
                actual,
            }) => {
                self.report_constraint_generic_argument_count_mismatch(
                    constraint_type,
                    constraint_name.as_str(),
                    expected,
                    actual,
                );
                return;
            }
        };

        let constraint_symbol = application.symbol;
        let constraint_name = application.constraint_name.clone();
        let substitution = application.substitution;

        let constraint_fields = application
            .imported_constraint
            .as_ref()
            .map(|constraint| self.imported_constraint_fields(constraint))
            .unwrap_or_else(|| self.constraint_fields(constraint_symbol));

        for constraint_field in constraint_fields {
            let Some(struct_field) = struct_fields
                .iter()
                .find(|field| field.name == constraint_field.name)
            else {
                self.report_missing_constraint_field(
                    struct_item,
                    struct_name,
                    constraint_name.as_str(),
                    constraint_field.name.as_str(),
                );
                continue;
            };

            let expected = self.substitute_type(constraint_field.ty, &substitution);

            if self.is_assignable(expected, struct_field.ty) {
                continue;
            }

            self.report_constraint_field_type_mismatch(
                struct_field.node,
                struct_name,
                constraint_name.as_str(),
                constraint_field.name.as_str(),
                expected,
                struct_field.ty,
            );
        }

        let constraint_functions = application
            .imported_constraint
            .as_ref()
            .map(|constraint| self.imported_constraint_functions(constraint))
            .unwrap_or_else(|| self.constraint_functions(constraint_symbol));

        for constraint_function in constraint_functions {
            let Some(struct_function) = struct_functions
                .iter()
                .find(|function| function.name == constraint_function.name)
            else {
                self.report_missing_constraint_function(
                    struct_item,
                    struct_name,
                    constraint_name.as_str(),
                    constraint_function.name.as_str(),
                );
                continue;
            };

            let expected = self.substitute_type(constraint_function.ty, &substitution);

            if self.is_assignable(expected, struct_function.ty) {
                continue;
            }

            self.report_constraint_function_type_mismatch(
                struct_function.node,
                struct_name,
                constraint_name.as_str(),
                constraint_function.name.as_str(),
                expected,
                struct_function.ty,
            );
        }
    }

    fn struct_item_symbol(&self, struct_item: NodeId) -> Option<(SymbolId, String)> {
        let name_node = self
            .graph
            .syntax()
            .first_child_of_kind(struct_item, SyntaxNodeKind::Identifier)?;

        let struct_name = self.node_text(name_node);
        let resolution = self.graph.resolution()?;

        let symbol = resolution
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == struct_name && symbol.kind() == SymbolKind::Struct)
            .map(|symbol| symbol.id())?;

        Some((symbol, struct_name))
    }

    pub(super) fn symbol_name(&self, symbol: SymbolId) -> Option<String> {
        let resolution = self.graph.resolution()?;
        let symbol_data = resolution.symbol(symbol)?;

        Some(symbol_data.name().to_string())
    }

    fn struct_satisfies_fields(&self, struct_symbol: SymbolId) -> Vec<StructFieldInfo> {
        let Some(resolution) = self.graph.resolution() else {
            return Vec::new();
        };

        let Some(member_scope) = resolution.member_scope(struct_symbol) else {
            return Vec::new();
        };

        let Some(scope) = resolution.scope(member_scope) else {
            return Vec::new();
        };

        scope
            .symbols()
            .iter()
            .filter_map(|(name, symbol)| {
                let symbol_data = resolution.symbol(*symbol)?;

                if symbol_data.kind() != SymbolKind::StructField {
                    return None;
                }

                let ty = self.layer.symbol_type(*symbol)?;

                Some(StructFieldInfo {
                    node: symbol_data.declaration(),
                    name: name.to_string(),
                    ty,
                })
            })
            .collect()
    }

    fn constraint_fields(&self, constraint_symbol: SymbolId) -> Vec<ConstraintFieldInfo> {
        let Some(resolution) = self.graph.resolution() else {
            return Vec::new();
        };

        let Some(member_scope) = resolution.member_scope(constraint_symbol) else {
            return Vec::new();
        };

        let Some(scope) = resolution.scope(member_scope) else {
            return Vec::new();
        };

        scope
            .symbols()
            .iter()
            .filter_map(|(name, symbol)| {
                let symbol_data = resolution.symbol(*symbol)?;

                if symbol_data.kind() != SymbolKind::ConstraintField {
                    return None;
                }

                let ty = self.layer.symbol_type(*symbol)?;

                Some(ConstraintFieldInfo {
                    name: name.to_string(),
                    ty,
                })
            })
            .collect()
    }

    fn constraint_functions(&self, constraint_symbol: SymbolId) -> Vec<ConstraintFunctionInfo> {
        let Some(resolution) = self.graph.resolution() else {
            return Vec::new();
        };

        let Some(member_scope) = resolution.member_scope(constraint_symbol) else {
            return Vec::new();
        };

        let Some(scope) = resolution.scope(member_scope) else {
            return Vec::new();
        };

        scope
            .symbols()
            .iter()
            .filter_map(|(name, symbol)| {
                let symbol_data = resolution.symbol(*symbol)?;

                if symbol_data.kind() != SymbolKind::ConstraintFunction {
                    return None;
                }

                let ty = self.layer.symbol_type(*symbol)?;

                Some(ConstraintFunctionInfo {
                    name: name.to_string(),
                    ty,
                })
            })
            .collect()
    }

    fn imported_constraint_fields(
        &self,
        constraint: &LoweredImportedConstraint,
    ) -> Vec<ConstraintFieldInfo> {
        constraint
            .fields
            .iter()
            .map(|field| ConstraintFieldInfo {
                name: field.name.clone(),
                ty: field.ty,
            })
            .collect()
    }

    fn imported_constraint_functions(
        &self,
        constraint: &LoweredImportedConstraint,
    ) -> Vec<ConstraintFunctionInfo> {
        constraint
            .functions
            .iter()
            .map(|function| ConstraintFunctionInfo {
                name: function.name.clone(),
                ty: function.ty,
            })
            .collect()
    }

    fn struct_satisfies_functions(&self, struct_name: &str) -> Vec<StructFunctionInfo> {
        let Some(root) = self.graph.syntax().root() else {
            return Vec::new();
        };

        let mut functions = Vec::new();
        self.collect_anchored_function_items(root, struct_name, &mut functions);

        functions
            .into_iter()
            .filter_map(|function| {
                let (_, function_name) = self.function_anchor_and_name(function)?;
                let symbol = self.function_item_symbol(function, function_name.as_str())?;
                let ty = self.layer.symbol_type(symbol)?;

                Some(StructFunctionInfo {
                    node: function,
                    name: function_name,
                    ty,
                })
            })
            .collect()
    }

    fn collect_anchored_function_items(
        &self,
        node: NodeId,
        struct_name: &str,
        functions: &mut Vec<NodeId>,
    ) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if syntax_node.kind() == SyntaxNodeKind::FunctionItem {
            if let Some((anchor_name, _)) = self.function_anchor_and_name(node)
                && anchor_name == struct_name
            {
                functions.push(node);
            }

            return;
        }

        for child in syntax_node.children() {
            self.collect_anchored_function_items(*child, struct_name, functions);
        }
    }

    fn function_anchor_and_name(&self, function: NodeId) -> Option<(String, String)> {
        let function_node = self.graph.syntax().node(function)?;
        let children = function_node.children();

        let anchor_index = children.iter().position(|child| {
            self.graph
                .syntax()
                .node(*child)
                .map(|node| node.kind() == SyntaxNodeKind::FunctionAnchor)
                .unwrap_or(false)
        })?;

        let anchor = *children.get(anchor_index)?;

        let name = children
            .iter()
            .skip(anchor_index + 1)
            .copied()
            .find(|child| {
                self.graph
                    .syntax()
                    .node(*child)
                    .map(|node| node.kind() == SyntaxNodeKind::Identifier)
                    .unwrap_or(false)
            })?;

        Some((
            self.function_anchor_base_name(anchor)?,
            self.node_text(name),
        ))
    }

    fn function_anchor_base_name(&self, anchor: NodeId) -> Option<String> {
        let anchor_type = self.graph.syntax().first_child(anchor)?;
        self.type_base_name(anchor_type)
    }

    fn type_base_name(&self, node: NodeId) -> Option<String> {
        let syntax_node = self.graph.syntax().node(node)?;

        match syntax_node.kind() {
            SyntaxNodeKind::NamedType | SyntaxNodeKind::Path => {
                let identifier = self
                    .graph
                    .syntax()
                    .first_child_of_kind(node, SyntaxNodeKind::Identifier)?;
                Some(self.node_text(identifier))
            }

            SyntaxNodeKind::GenericType => {
                let base = self.graph.syntax().first_child(node)?;
                self.type_base_name(base)
            }

            _ => None,
        }
    }

    fn function_item_symbol(&self, function: NodeId, function_name: &str) -> Option<SymbolId> {
        let resolution = self.graph.resolution()?;

        if let Some(symbol) = resolution.declaration_symbol(function) {
            let symbol_data = resolution.symbol(symbol)?;

            if symbol_data.kind() == SymbolKind::Function {
                return Some(symbol);
            }
        }

        let function_node = self.graph.syntax().node(function)?;

        for child in function_node.children() {
            let Some(child_node) = self.graph.syntax().node(*child) else {
                continue;
            };

            if child_node.kind() != SyntaxNodeKind::Identifier {
                continue;
            }

            if self.node_text(*child) != function_name {
                continue;
            }

            let Some(symbol) = resolution.declaration_symbol(*child) else {
                continue;
            };

            let Some(symbol_data) = resolution.symbol(symbol) else {
                continue;
            };

            if symbol_data.kind() == SymbolKind::Function {
                return Some(symbol);
            }
        }

        resolution
            .symbols()
            .iter()
            .find(|symbol| {
                symbol.kind() == SymbolKind::Function
                    && (symbol.declaration() == function
                        || symbol.name() == function_name
                        || symbol
                            .name()
                            .ends_with(format!("::{function_name}").as_str()))
            })
            .map(|symbol| symbol.id())
    }
}
