use std::collections::HashMap;

use super::MirBuilder;
use galfus_core::{NodeId, SymbolId, TypeId};
use galfus_frontend::{SyntaxNodeKind, TypeKind};

impl<'a> MirBuilder<'a> {
    pub fn generic_parameters_for_function_item(&self, item: NodeId) -> Vec<SymbolId> {
        let syntax = self.graph.syntax();
        let Some(generic_list) =
            syntax.first_child_of_kind(item, SyntaxNodeKind::GenericParameterList)
        else {
            return Vec::new();
        };

        let Some(generic_node) = syntax.node(generic_list) else {
            return Vec::new();
        };

        generic_node
            .children()
            .iter()
            .filter_map(|parameter| {
                let identifier =
                    syntax.first_child_of_kind(*parameter, SyntaxNodeKind::Identifier)?;
                self.graph
                    .resolution()
                    .and_then(|resolution| resolution.declaration_symbol(identifier))
            })
            .collect()
    }

    pub(super) fn requires_specialization(&self, item: NodeId) -> bool {
        !self.generic_parameters_for_function_item(item).is_empty()
            || !self
                .generic_parameters_for_anchored_function_item(item)
                .is_empty()
    }

    pub fn anchored_function_specializations_for_type(
        &self,
        ty: TypeId,
        substitutions: &HashMap<SymbolId, TypeId>,
    ) -> Vec<(NodeId, Vec<TypeId>, HashMap<SymbolId, TypeId>)> {
        let ty = self.resolve_alias_type(ty);
        let Some(TypeKind::GenericInstance { base, arguments }) =
            self.type_result.layer().table().kind(ty).cloned()
        else {
            return Vec::new();
        };

        let mut functions = Vec::new();
        let Some(root) = self.graph.syntax().root() else {
            return functions;
        };
        let arguments = arguments
            .iter()
            .map(|argument| self.substitute_type(*argument, substitutions))
            .collect::<Vec<_>>();
        self.collect_anchored_function_specializations(root, base, &arguments, &mut functions);
        functions
    }

    pub fn function_symbol_for_item(&self, item: NodeId) -> Option<SymbolId> {
        let name = self.function_name_node(item)?;
        self.graph.resolution()?.declaration_symbol(name)
    }

    fn collect_anchored_function_specializations(
        &self,
        node: NodeId,
        base: TypeId,
        arguments: &[TypeId],
        functions: &mut Vec<(NodeId, Vec<TypeId>, HashMap<SymbolId, TypeId>)>,
    ) {
        let syntax = self.graph.syntax();
        let Some(syntax_node) = syntax.node(node) else {
            return;
        };

        if syntax_node.kind() == SyntaxNodeKind::FunctionItem {
            let generic_params = self.generic_parameters_for_anchored_function_item(node);
            if generic_params.len() == arguments.len()
                && !generic_params.is_empty()
                && let Some(anchor) = syntax.first_child_of_kind(node, SyntaxNodeKind::FunctionAnchor)
                && let Some(anchor_type) = syntax.first_child(anchor)
                && self.type_result.layer().node_type(anchor_type).is_some_and(|anchor_ty| {
                    matches!(
                        self.type_result.layer().table().kind(self.resolve_alias_type(anchor_ty)),
                        Some(TypeKind::GenericInstance { base: anchor_base, .. }) if *anchor_base == base
                    )
                })
            {
                let substitutions = generic_params
                    .into_iter()
                    .zip(arguments.iter().copied())
                    .collect::<HashMap<_, _>>();
                functions.push((node, arguments.to_vec(), substitutions));
            }
        }

        for &child in syntax_node.children() {
            self.collect_anchored_function_specializations(child, base, arguments, functions);
        }
    }

    fn generic_parameters_for_anchored_function_item(&self, item: NodeId) -> Vec<SymbolId> {
        let Some(symbol) = self.function_symbol_for_item(item) else {
            return Vec::new();
        };
        let Some(function_type) = self.type_result.layer().symbol_type(symbol) else {
            return Vec::new();
        };
        let mut parameters = Vec::new();
        self.collect_generic_parameters_from_type(function_type, &mut parameters);
        parameters
    }

    fn collect_generic_parameters_from_type(&self, ty: TypeId, parameters: &mut Vec<SymbolId>) {
        match self.type_result.layer().table().kind(ty) {
            Some(TypeKind::GenericParameter { symbol }) => {
                if !parameters.contains(symbol) {
                    parameters.push(*symbol);
                }
            }
            Some(TypeKind::Array { element }) | Some(TypeKind::Range { element }) => {
                self.collect_generic_parameters_from_type(*element, parameters);
            }
            Some(TypeKind::Tuple { elements }) | Some(TypeKind::Union { members: elements }) => {
                for element in elements {
                    self.collect_generic_parameters_from_type(*element, parameters);
                }
            }
            Some(TypeKind::Function(function)) => {
                for parameter in function.parameters() {
                    self.collect_generic_parameters_from_type(parameter.ty(), parameters);
                }
                self.collect_generic_parameters_from_type(function.return_type(), parameters);
            }
            Some(TypeKind::GenericInstance { base, arguments }) => {
                self.collect_generic_parameters_from_type(*base, parameters);
                for argument in arguments {
                    self.collect_generic_parameters_from_type(*argument, parameters);
                }
            }
            _ => {}
        }
    }

    pub(super) fn substitute_type(
        &self,
        ty: TypeId,
        substitutions: &HashMap<SymbolId, TypeId>,
    ) -> TypeId {
        let ty = self.resolve_alias_type(ty);

        match self.type_result.layer().table().kind(ty) {
            Some(TypeKind::GenericParameter { symbol }) => {
                substitutions.get(symbol).copied().unwrap_or(ty)
            }
            _ => ty,
        }
    }
}
