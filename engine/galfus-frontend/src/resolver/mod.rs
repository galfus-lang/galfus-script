mod anchor;
mod block;
mod builtin;
mod export;
mod function;
mod generic;
mod import;
mod reference;
mod resolution;
mod scope;
mod symbol;
#[cfg(test)]
mod tests;
mod type_member;
mod type_reference;

use crate::{AsNameId, ModuleAst, NameId, ResolverDiagnosticCode, SyntaxLayer, SyntaxNodeKind};
pub use export::*;
use galfus_core::{Diagnostic, DiagnosticBag, NodeId, ScopeId, SourceFile, Span, SymbolId};
pub use import::*;
pub use resolution::*;
pub use scope::*;
pub use symbol::*;

pub struct ResolveResult {
    graph: ModuleAst,
}

impl ResolveResult {
    pub fn ast(&self) -> &ModuleAst {
        &self.graph
    }

    pub fn into_ast(self) -> ModuleAst {
        self.graph
    }

    pub fn graph(&self) -> &ModuleAst {
        self.ast()
    }

    pub fn into_graph(self) -> ModuleAst {
        self.into_ast()
    }

    pub fn has_errors(&self) -> bool {
        self.graph.has_errors()
    }

    pub fn diagnostics(&self) -> &DiagnosticBag {
        self.graph.diagnostics()
    }
}

pub struct Resolver<'a> {
    source: &'a SourceFile,
    syntax: &'a SyntaxLayer,
    diagnostics: DiagnosticBag,

    resolution: ResolutionLayer,
}

impl<'a> Resolver<'a> {
    pub fn new(source: &'a SourceFile, syntax: &'a SyntaxLayer) -> Self {
        let resolution = ResolutionLayer::new();

        let mut resolver = Self {
            source,
            syntax,
            diagnostics: DiagnosticBag::new(),
            resolution,
        };

        resolver.create_builtin_scope();
        resolver
            .resolution
            .add_scope(ScopeKind::Module, None, syntax.root());

        resolver
    }

    pub fn resolve(mut self) -> (ResolutionLayer, DiagnosticBag) {
        self.resolve_source_file();

        (self.resolution, self.diagnostics)
    }

    fn resolve_source_file(&mut self) {
        let Some(root) = self.syntax.root() else {
            return;
        };

        let module_scope = self.resolution.module_scope();
        self.resolution.bind_scope(root, module_scope);

        let Some(root_node) = self.syntax.node(root) else {
            return;
        };

        for item in root_node.children() {
            self.declare_top_level_item(*item, module_scope);
        }

        for item in root_node.children() {
            self.declare_import_item(*item, module_scope);
        }

        for item in root_node.children() {
            self.build_export_surface_item(*item);
        }

        for item in root_node.children() {
            self.resolve_function_scope_item(*item, module_scope);
        }

        for item in root_node.children() {
            self.resolve_generic_parameter_scope_item(*item, module_scope);
        }

        for item in root_node.children() {
            self.resolve_block_scope_item(*item, module_scope);
        }

        for item in root_node.children() {
            self.resolve_type_reference_item(*item, module_scope);
        }

        for item in root_node.children() {
            self.resolve_type_member_scope_item(*item, module_scope);
        }

        for item in root_node.children() {
            self.resolve_type_path_member_item(*item, module_scope);
        }

        for item in root_node.children() {
            self.validate_function_anchor_item(*item);
        }

        for item in root_node.children() {
            self.resolve_reference_item(*item, module_scope);
        }
    }

    fn declare_top_level_item(&mut self, item: NodeId, scope: ScopeId) {
        let Some(node) = self.syntax.node(item) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::WildcardPattern => {}

            SyntaxNodeKind::ExportItem => {
                if let Some(inner) = node.first_child() {
                    self.declare_top_level_item(inner, scope);
                }
            }

            SyntaxNodeKind::ImportItem => {
                // R1 intentionally ignores imports.
                // R2 will declare local import bindings.
            }

            SyntaxNodeKind::FunctionItem => {
                self.declare_function_item(item, scope);
            }

            SyntaxNodeKind::TypeAliasItem => {
                self.declare_named_item(item, SymbolKind::TypeAlias, scope);
            }

            SyntaxNodeKind::StructItem => {
                self.declare_named_item(item, SymbolKind::Struct, scope);
            }

            SyntaxNodeKind::EnumItem => {
                self.declare_named_item(item, SymbolKind::Enum, scope);
            }

            SyntaxNodeKind::ChoiceItem => {
                self.declare_named_item(item, SymbolKind::Choice, scope);
            }

            SyntaxNodeKind::ConstraintItem => {
                self.declare_named_item(item, SymbolKind::Constraint, scope);
            }

            SyntaxNodeKind::VarItem => {
                self.declare_binding_item(item, SymbolKind::Var, scope);
            }

            SyntaxNodeKind::ConstItem => {
                self.declare_binding_item(item, SymbolKind::Const, scope);
            }

            _ => {}
        }
    }

    fn declare_function_item(&mut self, item: NodeId, scope: ScopeId) {
        let Some(name) = self.function_name(item) else {
            return;
        };

        let symbol_name = self.function_symbol_name(item, name);
        let name_id = NameId::intern(&symbol_name);

        self.declare_symbol(name_id, SymbolKind::Function, name, scope);
    }

    fn declare_named_item(&mut self, item: NodeId, kind: SymbolKind, scope: ScopeId) {
        let Some(name) = self
            .syntax
            .first_child_of_kind(item, SyntaxNodeKind::Identifier)
        else {
            return;
        };

        let symbol_name = self.node_text(name);
        let name_id = NameId::intern(&symbol_name);

        self.declare_symbol(name_id, kind, name, scope);
    }

    pub(super) fn function_name(&self, item: NodeId) -> Option<NodeId> {
        self.syntax
            .first_child_of_kind(item, SyntaxNodeKind::Identifier)
    }

    pub(super) fn function_symbol_name(&self, item: NodeId, name: NodeId) -> String {
        let Some(anchor) = self
            .syntax
            .first_child_of_kind(item, SyntaxNodeKind::FunctionAnchor)
        else {
            return self.node_text(name);
        };

        format!("{}::{}", self.node_text(anchor), self.node_text(name))
    }

    fn declare_binding_item(&mut self, item: NodeId, kind: SymbolKind, scope: ScopeId) {
        let Some(binding) = self
            .syntax
            .first_child_of_kind(item, SyntaxNodeKind::BindingPattern)
        else {
            return;
        };

        self.declare_binding_pattern(binding, kind, scope);
    }

    pub(super) fn declare_binding_pattern(
        &mut self,
        pattern: NodeId,
        kind: SymbolKind,
        scope: ScopeId,
    ) {
        let Some(node) = self.syntax.node(pattern) else {
            return;
        };

        match node.kind() {
            SyntaxNodeKind::WildcardPattern => {}

            SyntaxNodeKind::BindingPattern => {
                if let Some(inner) = node.first_child() {
                    self.declare_binding_pattern(inner, kind, scope);
                }
            }

            SyntaxNodeKind::Identifier => {
                let symbol_name = self.node_text(pattern);
                let name_id = NameId::intern(&symbol_name);
                self.declare_symbol(name_id, kind, pattern, scope);
            }

            SyntaxNodeKind::StructBindingPattern => {
                for field in node.children() {
                    self.declare_binding_pattern(*field, kind, scope);
                }
            }

            SyntaxNodeKind::StructPattern => {
                if node.children().len() > 1 {
                    for field in &node.children()[1..] {
                        self.declare_binding_pattern(*field, kind, scope);
                    }
                }
            }

            SyntaxNodeKind::StructBindingField | SyntaxNodeKind::StructPatternField => {
                match node.child_count() {
                    0 => {}

                    1 => {
                        if let Some(name) = node.first_child() {
                            let symbol_name = self.node_text(name);
                            let name_id = NameId::intern(&symbol_name);
                            self.declare_symbol(name_id, kind, name, scope);
                        }
                    }

                    _ => {
                        if let Some(alias_pattern) = node.child(1) {
                            self.declare_binding_pattern(alias_pattern, kind, scope);
                        }
                    }
                }
            }

            SyntaxNodeKind::TupleBindingPattern | SyntaxNodeKind::ArrayBindingPattern => {
                for child in node.children() {
                    self.declare_binding_pattern(*child, kind, scope);
                }
            }

            SyntaxNodeKind::RestBindingPattern => {
                if let Some(inner) = node.first_child() {
                    self.declare_binding_pattern(inner, kind, scope);
                }
            }

            _ => {}
        }
    }

    pub(super) fn declare_symbol<N: AsNameId>(
        &mut self,
        name: N,
        kind: SymbolKind,
        declaration: NodeId,
        scope: ScopeId,
    ) -> Option<SymbolId> {
        let name_id = name.to_name_id();
        if self
            .resolution
            .scope(scope)
            .and_then(|scope| scope.symbol(name_id))
            .is_some()
        {
            let span = self
                .syntax
                .node(declaration)
                .map(|node| node.span())
                .unwrap_or_else(|| Span::new(self.source.id(), 0, 0));

            self.diagnostics.push(Diagnostic::error_with_message(
                ResolverDiagnosticCode::DuplicateSymbol,
                format!("duplicate symbol `{name_id}`"),
                span,
            ));

            return None;
        }

        let symbol = self
            .resolution
            .add_symbol(kind, name_id, declaration, scope);

        if let Some(scope) = self.resolution.scope_mut(scope) {
            scope.insert_symbol(name_id, symbol);
        }

        Some(symbol)
    }

    pub(super) fn node_text(&self, node: NodeId) -> String {
        let Some(node) = self.syntax.node(node) else {
            return String::new();
        };

        self.source.slice(node.span()).unwrap_or("").to_string()
    }
}

pub fn resolve(source: &SourceFile, mut graph: ModuleAst) -> ResolveResult {
    let resolver = Resolver::new(source, graph.syntax());
    let (resolution, diagnostics) = resolver.resolve();

    graph.extend_diagnostics(diagnostics.into_vec());
    graph.set_resolution(resolution);

    ResolveResult { graph }
}
