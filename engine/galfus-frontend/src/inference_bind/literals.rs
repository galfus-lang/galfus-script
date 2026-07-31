

use galfus_core::{NodeId, TypeId};
use crate::{PrimitiveType, SyntaxNodeKind, TypeKind};
use super::ExpressionInferrer;

#[derive(Debug, Clone, Copy)]
struct ArrayLiteralElementType {
    ty: TypeId,
    is_dynamic: bool,
    has_error: bool,
}

impl<'a> ExpressionInferrer<'a> {
    pub(super) fn infer_string_literal_type(&mut self, node: NodeId) -> Option<TypeId> {
        let uint8_type = self.layer.table().primitive(PrimitiveType::Uint8);
        let ty = self.layer.table_mut().intern_array(uint8_type);

        self.layer.bind_node_type(node, ty);
        Some(ty)
    }

    pub(super) fn infer_array_literal_type(
        &mut self,
        node: NodeId,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let elements = self.graph.syntax().node(node)?.children().to_vec();

        if elements.is_empty() {
            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);

            return Some(error);
        }

        let expected_element_type = expected.and_then(|expected_ty| {
            let resolved = self.resolve_alias_type(expected_ty);
            match self.layer.table().kind(resolved) {
                Some(TypeKind::Array { element }) => Some(*element),
                _ => None,
            }
        });

        let mut element_types = Vec::new();

        for element in elements {
            let Some(element_type) =
                self.infer_array_literal_element_type(element, expected_element_type)
            else {
                continue;
            };

            element_types.push(element_type);
        }

        let expected_element_type = expected_element_type.or_else(|| {
            element_types
                .iter()
                .find(|element| !element.has_error)
                .map(|element| element.ty)
        });

        let Some(expected_element_type) = expected_element_type else {
            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);

            return Some(error);
        };

        let mut has_error = element_types.iter().any(|element| element.has_error);

        for element in element_types.iter().copied() {
            if element.has_error {
                continue;
            }

            if self.is_assignable(expected_element_type, element.ty) {
                continue;
            }
            has_error = true;
        }

        let is_dynamic = element_types.iter().any(|element| element.is_dynamic);

        if !is_dynamic && element_types.is_empty() {
            let error = self.layer.table_mut().error();
            self.layer.bind_node_type(node, error);

            return Some(error);
        }

        let ty = if has_error {
            self.layer.table_mut().error()
        } else {
            self.layer.table_mut().intern_array(expected_element_type)
        };

        self.layer.bind_node_type(node, ty);
        Some(ty)
    }

    fn infer_array_literal_element_type(
        &mut self,
        element: NodeId,
        expected_element_type: Option<TypeId>,
    ) -> Option<ArrayLiteralElementType> {
        let element_node = self.graph.syntax().node(element)?;

        match element_node.kind() {
            SyntaxNodeKind::ArrayElement => {
                let expression = self.graph.syntax().child(element, 0)?;
                let ty =
                    self.infer_expression_type_with_expected(expression, expected_element_type)?;

                self.layer.bind_node_type(element, ty);

                Some(ArrayLiteralElementType {
                    ty,
                    is_dynamic: false,
                    has_error: false,
                })
            }

            SyntaxNodeKind::SpreadArrayElement => {
                let expression = self.graph.syntax().child(element, 0)?;
                let expected_spread_type =
                    expected_element_type.map(|t| self.layer.table_mut().intern_array(t));
                let spread_type =
                    self.infer_expression_type_with_expected(expression, expected_spread_type)?;

                let result = self.array_literal_spread_element_type(expression, spread_type)?;

                self.layer.bind_node_type(element, result.ty);

                Some(result)
            }

            _ => {
                let ty =
                    self.infer_expression_type_with_expected(element, expected_element_type)?;

                Some(ArrayLiteralElementType {
                    ty,
                    is_dynamic: false,
                    has_error: false,
                })
            }
        }
    }

    fn array_literal_spread_element_type(
        &mut self,
        expression: NodeId,
        spread_type: TypeId,
    ) -> Option<ArrayLiteralElementType> {
        let resolved = self.resolve_alias_type(spread_type);

        match self.layer.table().kind(resolved).cloned() {
            Some(TypeKind::Array { .. }) => {
                if let Some(expr_node) = self.graph.syntax().node(expression)
                    && expr_node.kind() == SyntaxNodeKind::StringLiteral
                {
                    let element = match self.layer.table().kind(resolved) {
                        Some(TypeKind::Array { element }) => element,
                        _ => &self.layer.table().primitive(PrimitiveType::Uint8),
                    };
                    return Some(ArrayLiteralElementType {
                        ty: *element,
                        is_dynamic: false,
                        has_error: false,
                    });
                }

                let element = match self.layer.table().kind(resolved) {
                    Some(TypeKind::Array { element }) => element,
                    _ => &self.layer.table().primitive(PrimitiveType::Uint8),
                };

                Some(ArrayLiteralElementType {
                    ty: *element,
                    is_dynamic: true,
                    has_error: false,
                })
            }

            Some(TypeKind::Error) => Some(ArrayLiteralElementType {
                ty: resolved,
                is_dynamic: false,
                has_error: true,
            }),

            _ => {
                let error = self.layer.table_mut().error();

                Some(ArrayLiteralElementType {
                    ty: error,
                    is_dynamic: false,
                    has_error: true,
                })
            }
        }
    }
}
