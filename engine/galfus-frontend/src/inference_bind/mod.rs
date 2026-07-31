

mod access;
mod assignability;
mod calls;
mod expressions;
mod generic_expressions;
mod inference_stubs;
mod inferred_structs;
mod instanceof;
mod literals;
mod operators;
mod ranges;
mod support;
mod typeofs;

use crate::types::TypeLayer;
use std::collections::HashMap;
use galfus_core::{NodeId, SourceFile, SymbolId, TypeId};
use crate::ImportedMemberKey;
use crate::{ImportedSurfaceTypes, ModuleAst};

pub fn infer_expressions(
    source: &SourceFile,
    graph: &ModuleAst,
    layer: TypeLayer,
    _imported_types: &ImportedSurfaceTypes,
) -> InferenceResult {
    let mut inferrer = ExpressionInferrer::new(source, graph, layer);

    // Visit all nodes to infer expressions
    if let Some(root) = graph.syntax().root() {
        inferrer.infer_declarations(root);
    }

    InferenceResult {
        layer: inferrer.layer,
    }
}

pub struct InferenceResult {
    pub layer: TypeLayer,
}

impl InferenceResult {
    pub fn into_layer(self) -> TypeLayer {
        self.layer
    }
}

pub struct ExpressionInferrer<'a> {
    pub(super) source: &'a SourceFile,
    pub(super) graph: &'a ModuleAst,
    pub(super) layer: TypeLayer,

    // Track bounds and constraints
    pub(super) imported_member_types: HashMap<ImportedMemberKey, TypeId>,
    pub(super) active_type_substitutions: Vec<HashMap<SymbolId, TypeId>>,
}

impl<'a> ExpressionInferrer<'a> {
    pub fn new(source: &'a SourceFile, graph: &'a ModuleAst, layer: TypeLayer) -> Self {
        Self {
            source,
            graph,
            layer,
            imported_member_types: HashMap::new(),
            active_type_substitutions: Vec::new(),
        }
    }

    pub fn infer_declarations(&mut self, root: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(root) else {
            return;
        };
        for child in syntax_node.children() {
            self.infer_expression_type(*child); // infer expressions recursively
        }
    }
}
