use super::DeclarationTypeChecker;
use crate::{FunctionParameterType, PrimitiveType, SymbolKind, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, TypeId};

fn primitive_type_by_name(name: &str) -> Option<PrimitiveType> {
    match name {
        "null" => Some(PrimitiveType::Null),
        "bool" => Some(PrimitiveType::Bool),
        "i8" => Some(PrimitiveType::Int8),
        "i16" => Some(PrimitiveType::Int16),
        "i32" => Some(PrimitiveType::Int32),
        "i64" => Some(PrimitiveType::Int64),
        "u8" => Some(PrimitiveType::Uint8),
        "u16" => Some(PrimitiveType::Uint16),
        "u32" => Some(PrimitiveType::Uint32),
        "u64" => Some(PrimitiveType::Uint64),
        "f16" => Some(PrimitiveType::Float16),
        "f32" => Some(PrimitiveType::Float32),
        "f64" => Some(PrimitiveType::Float64),
        _ => None,
    }
}

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn bind_builtin_symbol_types(&mut self) {
        let Some(resolution) = self.graph.resolution() else {
            return;
        };

        for symbol in resolution.symbols() {
            if symbol.kind() != SymbolKind::BuiltinType {
                continue;
            }

            if let Some(ty) = self.layer.table_mut().primitive_family(symbol.name()) {
                self.layer.bind_symbol_type(symbol.id(), ty);
                continue;
            }

            let Some(primitive) = primitive_type_by_name(symbol.name()) else {
                continue;
            };

            let ty = self.layer.table().primitive(primitive);
            self.layer.bind_symbol_type(symbol.id(), ty);
        }
    }

    pub(super) fn bind_named_type_definition_symbols(&mut self) {
        let Some(resolution) = self.graph.resolution() else {
            return;
        };

        let symbols = resolution.symbols().to_vec();

        for symbol in symbols {
            match symbol.kind() {
                SymbolKind::Struct
                | SymbolKind::Enum
                | SymbolKind::Choice
                | SymbolKind::Constraint => {
                    let ty = self.layer.table_mut().intern_named(symbol.id());
                    self.layer.bind_symbol_type(symbol.id(), ty);
                }

                _ => {}
            }
        }
    }

    pub(super) fn bind_node_types(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        match syntax_node.kind() {
            SyntaxNodeKind::FunctionItem => {
                self.bind_function_item_type(node);
            }

            SyntaxNodeKind::ConstraintFunctionSignature => {
                self.bind_constraint_function_signature_type(node);
            }

            SyntaxNodeKind::Parameter | SyntaxNodeKind::RestParameter => {
                let Some(type_node) = self.first_type_child(node) else {
                    return;
                };

                let Some(ty) = self.layer.node_type(type_node) else {
                    return;
                };

                if let Some(pattern) = self
                    .graph
                    .syntax()
                    .first_child_of_kind(node, SyntaxNodeKind::BindingPattern)
                {
                    self.bind_binding_pattern_type(pattern, ty);
                } else if let Some(symbol) = self.direct_identifier_symbol_any(
                    node,
                    &[SymbolKind::Parameter, SymbolKind::RestParameter],
                ) {
                    self.layer.bind_symbol_type(symbol, ty);
                }
            }

            SyntaxNodeKind::StructField | SyntaxNodeKind::WeakStructField => {
                self.bind_direct_declaration_type(node, &[SymbolKind::StructField]);
            }

            SyntaxNodeKind::ConstraintField => {
                self.bind_direct_declaration_type(node, &[SymbolKind::ConstraintField]);
            }

            SyntaxNodeKind::VarItem
            | SyntaxNodeKind::ConstItem
            | SyntaxNodeKind::VarStatement
            | SyntaxNodeKind::ConstStatement => {
                self.bind_binding_declaration_type(node);
            }

            SyntaxNodeKind::TypeAliasItem => {
                self.bind_type_alias_type(node);
            }

            SyntaxNodeKind::GenericParameter => {
                self.bind_generic_parameter_type(node);
            }

            _ => {}
        }

        for child in syntax_node.children() {
            self.bind_node_types(*child);
        }
    }

    fn bind_function_item_type(&mut self, node: NodeId) {
        let Some(symbol) = self.direct_identifier_symbol(node, SymbolKind::Function) else {
            return;
        };

        let Some(ty) = self.lower_function_signature_type(node) else {
            return;
        };

        self.layer.bind_symbol_type(symbol, ty);
        self.bind_function_parameter_symbols(node, ty);
    }

    fn bind_function_parameter_symbols(&mut self, node: NodeId, function_ty: TypeId) {
        let Some(parameters_node) = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::ParameterList)
        else {
            return;
        };

        let Some(TypeKind::Function(function)) = self.layer.table().kind(function_ty).cloned()
        else {
            return;
        };

        let parameter_nodes = self
            .graph
            .syntax()
            .node(parameters_node)
            .map(|n| n.children().to_vec())
            .unwrap_or_default();

        let mut param_idx = 0usize;
        for parameter in parameter_nodes {
            let kind = self.graph.syntax().node(parameter).map(|n| n.kind());
            if !matches!(
                kind,
                Some(SyntaxNodeKind::Parameter | SyntaxNodeKind::RestParameter)
            ) {
                continue;
            }

            if self.is_self_parameter(parameter)
                && let Some(param_type) = function.parameters().get(param_idx)
            {
                let ty = param_type.ty();
                if let Some(self_symbol) =
                    self.direct_identifier_symbol_any(parameter, &[SymbolKind::Parameter])
                {
                    self.layer.bind_symbol_type(self_symbol, ty);
                }
            }

            param_idx += 1;
        }
    }

    fn bind_constraint_function_signature_type(&mut self, node: NodeId) {
        let Some(symbol) = self.direct_identifier_symbol(node, SymbolKind::ConstraintFunction)
        else {
            return;
        };

        let Some(ty) = self.lower_function_signature_type(node) else {
            return;
        };

        self.layer.bind_symbol_type(symbol, ty);
    }

    fn lower_function_signature_type(&mut self, node: NodeId) -> Option<TypeId> {
        let parameters_node = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::ParameterList)?;

        let return_type_node = self.last_direct_type_child(node)?;

        let parameters = self
            .graph
            .syntax()
            .node(parameters_node)?
            .children()
            .iter()
            .filter_map(|parameter| self.lower_function_parameter_type(*parameter, node))
            .collect::<Vec<_>>();

        let return_type = self.layer.node_type(return_type_node)?;

        Some(
            self.layer
                .table_mut()
                .intern_function(parameters, return_type),
        )
    }

    fn lower_function_parameter_type(
        &mut self,
        parameter: NodeId,
        signature: NodeId,
    ) -> Option<FunctionParameterType> {
        let parameter_node = self.graph.syntax().node(parameter)?;

        match parameter_node.kind() {
            SyntaxNodeKind::Parameter | SyntaxNodeKind::RestParameter => {}

            _ => return None,
        }

        let ty = match self.first_type_child(parameter) {
            Some(type_node) => self.layer.node_type(type_node)?,
            None if self.is_self_parameter(parameter) => self.infer_self_parameter_type(signature),
            None => return None,
        };

        let has_default = self
            .graph
            .syntax()
            .first_child_of_kind(parameter, SyntaxNodeKind::ParameterDefault)
            .is_some();

        if parameter_node.kind() == SyntaxNodeKind::RestParameter {
            return Some(FunctionParameterType::rest(ty));
        }

        if has_default {
            return Some(FunctionParameterType::with_default(ty));
        }

        Some(FunctionParameterType::new(ty))
    }

    fn is_self_parameter(&self, parameter: NodeId) -> bool {
        self.graph
            .syntax()
            .first_child_of_kind(parameter, SyntaxNodeKind::Identifier)
            .is_some_and(|name| self.node_text(name) == "self")
    }

    fn infer_self_parameter_type(&mut self, signature: NodeId) -> TypeId {
        let Some(signature_node) = self.graph.syntax().node(signature) else {
            return self.layer.table_mut().error();
        };

        if signature_node.kind() == SyntaxNodeKind::ConstraintFunctionSignature {
            return self.layer.table_mut().error();
        }

        let Some(anchor) = self
            .graph
            .syntax()
            .first_child_of_kind(signature, SyntaxNodeKind::FunctionAnchor)
        else {
            return self.layer.table_mut().error();
        };

        let Some(anchor_type) = self.graph.syntax().first_child(anchor) else {
            return self.layer.table_mut().error();
        };

        if self
            .graph
            .syntax()
            .node(anchor_type)
            .is_some_and(|node| node.kind() == SyntaxNodeKind::GenericType)
            && let Some(ty) = self.layer.node_type(anchor_type)
        {
            return ty;
        }

        let Some(base_type) = self.layer.node_type(anchor_type) else {
            return self.layer.table_mut().error();
        };

        let parameters = self.anchor_struct_generic_parameters(anchor_type);

        if parameters.is_empty() {
            return base_type;
        }

        let arguments = parameters
            .into_iter()
            .filter_map(|parameter| self.layer.symbol_type(parameter))
            .collect::<Vec<_>>();

        if arguments.is_empty() {
            return base_type;
        }

        self.layer
            .table_mut()
            .intern_generic_instance(base_type, arguments)
    }

    fn anchor_struct_generic_parameters(&self, anchor_type: NodeId) -> Vec<galfus_core::SymbolId> {
        let Some(resolution) = self.graph.resolution() else {
            return Vec::new();
        };

        let symbol = resolution
            .type_reference_symbol(anchor_type)
            .or_else(|| resolution.type_path_reference_symbol(anchor_type));

        let Some(symbol) = symbol else {
            return Vec::new();
        };

        let Some(struct_item) = self.type_item_for_symbol(symbol) else {
            return Vec::new();
        };

        self.declaration_symbols_in_node(struct_item, &[SymbolKind::GenericParameter])
    }

    fn bind_direct_declaration_type(&mut self, node: NodeId, kinds: &[SymbolKind]) {
        let Some(symbol) = self.direct_identifier_symbol_any(node, kinds) else {
            return;
        };

        let Some(type_node) = self.first_type_child(node) else {
            return;
        };

        let Some(ty) = self.layer.node_type(type_node) else {
            return;
        };

        self.layer.bind_symbol_type(symbol, ty);
    }

    fn bind_binding_declaration_type(&mut self, node: NodeId) {
        let Some(type_annotation) = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::TypeAnnotation)
        else {
            return;
        };

        let Some(type_node) = self.first_type_child(type_annotation) else {
            return;
        };

        let Some(ty) = self.layer.node_type(type_node) else {
            return;
        };

        if let Some(pattern) = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::BindingPattern)
        {
            self.bind_binding_pattern_type(pattern, ty);
            return;
        }

        let symbols = self.declaration_symbols_in_node(
            node,
            &[
                SymbolKind::Var,
                SymbolKind::Const,
                SymbolKind::PatternBinding,
                SymbolKind::TypePatternBinding,
            ],
        );

        for symbol in symbols {
            self.layer.bind_symbol_type(symbol, ty);
        }
    }

    pub(super) fn bind_binding_pattern_type(&mut self, pattern: NodeId, ty: TypeId) {
        let Some(pattern_node) = self.graph.syntax().node(pattern) else {
            return;
        };

        self.layer.bind_node_type(pattern, ty);

        match pattern_node.kind() {
            SyntaxNodeKind::BindingPattern => {
                if let Some(inner) = pattern_node.first_child() {
                    self.bind_binding_pattern_type(inner, ty);
                }
            }

            SyntaxNodeKind::Identifier => {
                if let Some(symbol) = self
                    .graph
                    .resolution()
                    .and_then(|resolution| resolution.declaration_symbol(pattern))
                {
                    self.layer.bind_symbol_type(symbol, ty);
                }
            }

            SyntaxNodeKind::StructBindingPattern => {
                self.bind_struct_binding_pattern_type(pattern, ty);
            }

            SyntaxNodeKind::StructBindingField => {
                self.bind_struct_binding_field_type(pattern, ty);
            }

            SyntaxNodeKind::TupleBindingPattern => {
                self.bind_tuple_binding_pattern_type(pattern, ty);
            }

            SyntaxNodeKind::ArrayBindingPattern => {
                self.bind_array_binding_pattern_type(pattern, ty);
            }

            SyntaxNodeKind::RestBindingPattern => {
                self.bind_rest_binding_pattern_type(pattern, ty);
            }

            SyntaxNodeKind::WildcardPattern => {}

            _ => {}
        }
    }

    fn bind_struct_binding_pattern_type(&mut self, pattern: NodeId, ty: TypeId) {
        let fields = self
            .graph
            .syntax()
            .node(pattern)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();

        for field in fields {
            self.bind_struct_binding_field_type(field, ty);
        }
    }

    fn bind_struct_binding_field_type(&mut self, field: NodeId, owner_type: TypeId) {
        let Some(name) = self.graph.syntax().child(field, 0) else {
            return;
        };

        let field_name = self.node_text(name);
        let Some(field_type) = self.member_type_for_target_type(owner_type, field_name.as_str())
        else {
            self.report_unknown_member(name, field_name.as_str(), owner_type);
            return;
        };

        match self.graph.syntax().child(field, 1) {
            Some(alias_pattern) => self.bind_binding_pattern_type(alias_pattern, field_type),
            None => {
                if let Some(symbol) = self
                    .graph
                    .resolution()
                    .and_then(|resolution| resolution.declaration_symbol(name))
                {
                    self.layer.bind_symbol_type(symbol, field_type);
                    self.layer.bind_node_type(field, field_type);
                }
            }
        }
    }

    fn bind_tuple_binding_pattern_type(&mut self, pattern: NodeId, ty: TypeId) {
        let elements = self
            .graph
            .syntax()
            .node(pattern)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();

        let resolved = self.resolve_alias_type(ty);
        let Some(TypeKind::Tuple {
            elements: element_types,
        }) = self.layer.table().kind(resolved).cloned()
        else {
            let error = self.layer.table_mut().error();
            self.report_type_mismatch(pattern, error, ty);
            return;
        };

        for (index, element) in elements.into_iter().enumerate() {
            let Some(element_type) = element_types.get(index).copied() else {
                continue;
            };

            self.bind_binding_pattern_type(element, element_type);
        }
    }

    fn bind_array_binding_pattern_type(&mut self, pattern: NodeId, ty: TypeId) {
        let elements = self
            .graph
            .syntax()
            .node(pattern)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();

        let Some(element_type) = self.array_binding_element_type(ty) else {
            let error = self.layer.table_mut().error();
            self.report_type_mismatch(pattern, error, ty);
            return;
        };

        for element in elements {
            let kind = self.graph.syntax().node(element).map(|node| node.kind());

            if kind == Some(SyntaxNodeKind::RestBindingPattern) {
                let rest_type = self.layer.table_mut().intern_array(element_type);
                self.bind_binding_pattern_type(element, rest_type);
                continue;
            }

            self.bind_binding_pattern_type(element, element_type);
        }
    }

    fn bind_rest_binding_pattern_type(&mut self, pattern: NodeId, ty: TypeId) {
        let Some(inner) = self.graph.syntax().child(pattern, 0) else {
            return;
        };

        self.bind_binding_pattern_type(inner, ty);
    }

    fn array_binding_element_type(&self, ty: TypeId) -> Option<TypeId> {
        let ty = self.resolve_alias_type(ty);

        match self.layer.table().kind(ty) {
            Some(TypeKind::Array { element }) => Some(*element),

            Some(TypeKind::Error) => Some(ty),

            _ => None,
        }
    }

    fn bind_type_alias_type(&mut self, node: NodeId) {
        let Some(type_node) = self.first_type_child(node) else {
            return;
        };

        let Some(ty) = self.layer.node_type(type_node) else {
            return;
        };

        let symbols = self.declaration_symbols_in_node(node, &[SymbolKind::TypeAlias]);

        for symbol in symbols {
            self.layer.bind_symbol_type(symbol, ty);
        }
    }

    fn bind_generic_parameter_type(&mut self, node: NodeId) {
        let Some(symbol) = self.direct_identifier_symbol(node, SymbolKind::GenericParameter) else {
            return;
        };

        let ty = self.layer.table_mut().intern_generic_parameter(symbol);
        self.layer.bind_symbol_type(symbol, ty);
    }
}
