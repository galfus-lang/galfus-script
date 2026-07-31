use super::DeclarationTypeChecker;
use crate::{PrimitiveType, SyntaxNodeKind, TypeKind};
use galfus_core::{NodeId, TypeId};

#[derive(Debug, Clone)]
pub(super) struct ControlTarget {
    pub(super) _node: NodeId,
    pub(super) name: Option<String>,
}

impl<'a> DeclarationTypeChecker<'a> {
    pub(super) fn check_control_flow(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        match syntax_node.kind() {
            SyntaxNodeKind::IfStatement => {
                self.check_if_statement_control_flow(node);
            }

            SyntaxNodeKind::LoopStatement => {
                self.check_loop_statement_control_flow(node);
            }

            SyntaxNodeKind::ForStatement => {
                self.check_for_statement_control_flow(node);
            }

            SyntaxNodeKind::BreakStatement => {
                let label = self.graph.syntax().child(node, 0);
                if let Some(lbl_node) = label {
                    let lbl_name = self.node_text(lbl_node);
                    if !self
                        .control_targets
                        .iter()
                        .any(|target| target.name.as_deref() == Some(&lbl_name))
                    {
                        self.report_unresolved_control_target(lbl_node, &lbl_name);
                    }
                } else if self.control_targets.is_empty() {
                    self.report_break_outside_loop(node);
                }
            }

            SyntaxNodeKind::ContinueStatement => {
                let label = self.graph.syntax().child(node, 0);
                if let Some(lbl_node) = label {
                    let lbl_name = self.node_text(lbl_node);
                    if !self
                        .control_targets
                        .iter()
                        .any(|target| target.name.as_deref() == Some(&lbl_name))
                    {
                        self.report_unresolved_control_target(lbl_node, &lbl_name);
                    }
                } else if self.control_targets.is_empty() {
                    self.report_continue_outside_loop(node);
                }
            }

            _ => {
                let children = syntax_node.children().to_vec();

                for child in children {
                    self.check_control_flow(child);
                }
            }
        }
    }

    fn check_if_statement_control_flow(&mut self, node: NodeId) {
        let Some(condition) = self.graph.syntax().child(node, 0) else {
            return;
        };

        self.check_bool_condition(condition);

        let children = self
            .graph
            .syntax()
            .node(node)
            .map(|node| node.children().to_vec())
            .unwrap_or_default();

        for child in children.into_iter().skip(1) {
            self.check_control_flow(child);
        }
    }

    fn check_loop_statement_control_flow(&mut self, node: NodeId) {
        let syntax = self.graph.syntax();
        if let Some(condition) = syntax.node(node).and_then(|loop_node| {
            loop_node.children().iter().copied().find(|&child| {
                let kind = syntax
                    .node(child)
                    .map(|c| c.kind())
                    .unwrap_or(SyntaxNodeKind::SourceFile);
                kind != SyntaxNodeKind::KeywordMetadataList && kind != SyntaxNodeKind::Block
            })
        }) {
            self.check_bool_condition(condition);
        }

        let target_name = self.loop_target_name(node);
        if let Some(ref name) = target_name
            && self
                .control_targets
                .iter()
                .any(|t| t.name.as_ref() == Some(name))
        {
            self.report_duplicate_control_target(node, name);
        }

        self.control_targets.push(ControlTarget {
            _node: node,
            name: target_name,
        });

        if let Some(body) = syntax.first_child_of_kind(node, SyntaxNodeKind::Block) {
            self.check_control_flow(body);
        }

        self.control_targets.pop();
    }

    fn check_for_statement_control_flow(&mut self, node: NodeId) {
        let syntax = self.graph.syntax();
        let Some(binding) = syntax.first_child_of_kind(node, SyntaxNodeKind::ForBinding) else {
            return;
        };

        let Some(body) = syntax.first_child_of_kind(node, SyntaxNodeKind::Block) else {
            return;
        };

        let Some(iterable) = syntax.node(node).and_then(|for_node| {
            for_node.children().iter().copied().find(|&child| {
                let kind = syntax
                    .node(child)
                    .map(|c| c.kind())
                    .unwrap_or(SyntaxNodeKind::SourceFile);
                kind != SyntaxNodeKind::KeywordMetadataList
                    && kind != SyntaxNodeKind::ForBinding
                    && kind != SyntaxNodeKind::Block
            })
        }) else {
            return;
        };

        let Some(element_type) = self.check_for_iterable_type(iterable) else {
            let target_name = self.loop_target_name(node);
            if let Some(ref name) = target_name
                && self
                    .control_targets
                    .iter()
                    .any(|t| t.name.as_ref() == Some(name))
            {
                self.report_duplicate_control_target(node, name);
            }
            self.control_targets.push(ControlTarget {
                _node: node,
                name: target_name,
            });
            self.check_control_flow(body);
            self.control_targets.pop();
            return;
        };

        self.bind_for_binding_type(binding, element_type);

        let target_name = self.loop_target_name(node);
        if let Some(ref name) = target_name
            && self
                .control_targets
                .iter()
                .any(|t| t.name.as_ref() == Some(name))
        {
            self.report_duplicate_control_target(node, name);
        }
        self.control_targets.push(ControlTarget {
            _node: node,
            name: target_name,
        });
        self.check_control_flow(body);
        self.control_targets.pop();
    }

    fn loop_target_name(&self, loop_node: NodeId) -> Option<String> {
        let metadata_list_node = self
            .graph
            .syntax()
            .first_child_of_kind(loop_node, SyntaxNodeKind::KeywordMetadataList)?;

        let metadata_list = self.graph.syntax().node(metadata_list_node)?;

        for child in metadata_list.children() {
            let child_node = self.graph.syntax().node(*child)?;
            if child_node.kind() == SyntaxNodeKind::KeywordMetadataPair
                && let Some(key_ident) = child_node.first_child()
                && self.node_text(key_ident) == "name"
                && let Some(val_ident) = self.graph.syntax().child(*child, 1)
            {
                return Some(self.node_text(val_ident).to_string());
            }
        }
        None
    }

    fn check_bool_condition(&mut self, condition: NodeId) {
        let Some(actual) = self.infer_expression_type(condition) else {
            return;
        };

        if self.is_bool_type(actual) {
            return;
        }

        self.report_invalid_condition_type(condition, actual);
    }

    fn is_bool_type(&self, ty: galfus_core::TypeId) -> bool {
        matches!(
            self.layer.table().kind(ty),
            Some(TypeKind::Primitive(PrimitiveType::Bool)) | Some(TypeKind::Error)
        )
    }

    fn check_for_iterable_type(&mut self, iterable: NodeId) -> Option<TypeId> {
        let actual = self.infer_expression_type(iterable)?;

        if let Some(element_type) = self.iterable_item_type(actual) {
            return Some(element_type);
        }

        self.report_invalid_iterable_type(iterable, actual);
        None
    }

    fn bind_for_binding_type(&mut self, binding: NodeId, element_type: TypeId) {
        let Some(binding_node) = self.graph.syntax().node(binding) else {
            return;
        };

        let index_type = self.layer.table().primitive(PrimitiveType::Int32);

        for (position, child) in binding_node.children().iter().copied().enumerate() {
            let Some(child_node) = self.graph.syntax().node(child) else {
                continue;
            };

            let binding_type = if position == 0 {
                element_type
            } else {
                index_type
            };

            self.layer.bind_node_type(child, binding_type);

            if child_node.kind() == SyntaxNodeKind::BindingPattern {
                self.bind_binding_pattern_type(child, binding_type);
            } else if child_node.kind() == SyntaxNodeKind::Identifier {
                if self.node_text(child) == "_" {
                    continue;
                }

                let symbol = self
                    .graph
                    .resolution()
                    .and_then(|resolution| resolution.declaration_symbol(child));

                if let Some(symbol) = symbol {
                    self.layer.bind_symbol_type(symbol, binding_type);
                }
            }
        }

        self.layer.bind_node_type(binding, element_type);
    }
}
