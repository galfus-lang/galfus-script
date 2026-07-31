use crate::modules::resolution::is_resolvable_import;
use crate::modules::session::{FrontendSession, PathCheckRecord, PathSegmentRecord};
use crate::{
    ImportKind, ImportedSurfaceTypes, ModuleSurface, SyntaxNodeKind,
    imported_surface_types_for_namespace,
};
use galfus_core::NodeId;

impl FrontendSession {
    pub(super) fn collect_named_imported_path_types(
        &self,
        module_index: usize,
        surfaces: &[ModuleSurface],
        imported_types: &mut ImportedSurfaceTypes,
    ) {
        let imports = self.module_imports(module_index);
        let named_imports = imports
            .iter()
            .filter(|import| {
                import.kind == ImportKind::Named && is_resolvable_import(import.source.as_str())
            })
            .collect::<Vec<_>>();

        if named_imports.is_empty() {
            return;
        }

        for path in self.path_records(module_index) {
            let Some(root_segment) = path.segments.first() else {
                continue;
            };

            let Some(import) = named_imports
                .iter()
                .find(|import| import.local_name == root_segment.name)
            else {
                continue;
            };

            let Some(imported_name) = import.imported_name.as_deref() else {
                continue;
            };

            let Some(target_index) = self.import_target_index(module_index, import.source.as_str())
            else {
                continue;
            };

            let member_name = path.segments[1..]
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>()
                .join("::");

            let Some(imported_type) = surfaces[target_index]
                .imported_member_path_type_for_named_export(
                    import.local_symbol,
                    imported_name,
                    &member_name,
                )
            else {
                continue;
            };

            imported_types.insert_path_type(path.node, imported_type);
        }
    }

    pub(super) fn collect_namespace_imported_path_types(
        &self,
        module_index: usize,
        surfaces: &[ModuleSurface],
        imported_types: &mut ImportedSurfaceTypes,
    ) {
        let imports = self.module_imports(module_index);
        let namespace_imports = imports
            .iter()
            .filter(|import| {
                import.kind == ImportKind::Namespace && is_resolvable_import(import.source.as_str())
            })
            .collect::<Vec<_>>();

        if namespace_imports.is_empty() {
            return;
        }

        for path in self.path_records(module_index) {
            let Some(root_segment) = path.segments.first() else {
                continue;
            };

            let Some(import) = namespace_imports
                .iter()
                .find(|import| import.local_name == root_segment.name)
            else {
                continue;
            };

            let Some(target_index) = self.import_target_index(module_index, import.source.as_str())
            else {
                continue;
            };

            imported_types.extend(imported_surface_types_for_namespace(
                &surfaces[target_index],
                import.local_symbol,
            ));

            let exported_name = path.segments[1..]
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>()
                .join("::");

            let Some(imported_type) = surfaces[target_index]
                .imported_path_type_for_export(import.local_symbol, &exported_name)
            else {
                if let Some(imported_constraint) =
                    surfaces[target_index].imported_constraint_for_export(exported_name.as_str())
                {
                    imported_types.insert_path_constraint(path.node, imported_constraint);
                }

                continue;
            };

            imported_types.insert_path_type(path.node, imported_type);

            if let Some(imported_constraint) =
                surfaces[target_index].imported_constraint_for_export(exported_name.as_str())
            {
                imported_types.insert_path_constraint(path.node, imported_constraint);
            }

            if let Some(imported_choice) =
                surfaces[target_index].imported_choice_for_export(exported_name.as_str())
            {
                imported_types.insert_path_choice(path.node, imported_choice);
            }
        }
    }

    fn path_records(&self, module_index: usize) -> Vec<PathCheckRecord> {
        let mut records = Vec::new();

        let Some(root) = self.modules[module_index].graph().syntax().root() else {
            return records;
        };

        self.collect_path_records(module_index, root, &mut records);

        records
    }

    fn collect_path_records(
        &self,
        module_index: usize,
        node: NodeId,
        records: &mut Vec<PathCheckRecord>,
    ) {
        let syntax = self.modules[module_index].graph().syntax();

        let Some(syntax_node) = syntax.node(node) else {
            return;
        };

        match syntax_node.kind() {
            SyntaxNodeKind::PathExpression => {
                let segments = self.path_expression_segments(module_index, node);

                if segments.len() >= 2 {
                    records.push(PathCheckRecord { node, segments });
                }

                return;
            }

            SyntaxNodeKind::Path => {
                let segments = self.type_path_segments(module_index, node);

                if segments.len() >= 2 {
                    records.push(PathCheckRecord { node, segments });
                }

                return;
            }

            _ => {}
        }

        for child in syntax_node.children() {
            self.collect_path_records(module_index, *child, records);
        }
    }

    fn path_expression_segments(
        &self,
        module_index: usize,
        expression: NodeId,
    ) -> Vec<PathSegmentRecord> {
        let syntax = self.modules[module_index].graph().syntax();

        let Some(expression_node) = syntax.node(expression) else {
            return Vec::new();
        };

        match expression_node.kind() {
            SyntaxNodeKind::NameExpression => {
                let Some(identifier) =
                    syntax.first_child_of_kind(expression, SyntaxNodeKind::Identifier)
                else {
                    return Vec::new();
                };

                vec![PathSegmentRecord {
                    name: self.node_text(module_index, identifier),
                }]
            }

            SyntaxNodeKind::PathExpression => {
                let Some(target) = syntax.child(expression, 0) else {
                    return Vec::new();
                };

                let Some(member) = syntax.child(expression, 1) else {
                    return Vec::new();
                };

                let mut segments = self.path_expression_segments(module_index, target);

                segments.push(PathSegmentRecord {
                    name: self.node_text(module_index, member),
                });

                segments
            }

            _ => Vec::new(),
        }
    }

    fn type_path_segments(&self, module_index: usize, path: NodeId) -> Vec<PathSegmentRecord> {
        let syntax = self.modules[module_index].graph().syntax();

        let Some(path_node) = syntax.node(path) else {
            return Vec::new();
        };

        path_node
            .children()
            .iter()
            .filter_map(|child| {
                let child_node = syntax.node(*child)?;

                if child_node.kind() != SyntaxNodeKind::Identifier {
                    return None;
                }

                Some(PathSegmentRecord {
                    name: self.node_text(module_index, *child),
                })
            })
            .collect()
    }

    fn node_text(&self, module_index: usize, node: NodeId) -> String {
        let Some(syntax_node) = self.modules[module_index].graph().syntax().node(node) else {
            return String::new();
        };

        self.modules[module_index]
            .source()
            .slice(syntax_node.span())
            .unwrap_or("")
            .to_string()
    }
}
