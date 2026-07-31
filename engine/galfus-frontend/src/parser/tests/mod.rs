mod anchored_functions;
mod binding_patterns;
mod constraint_items;
mod decorators;
mod default_parameters;
mod expressions;
mod function_types;
mod functions;
mod generic_constraints;
mod generic_declarations;
mod generic_types;
mod grouped_types;
mod module_items;
mod parser_core;
mod ranges;
mod rest_parameters;
mod statements;
mod struct_and_choice_items;
mod struct_fields;
mod struct_satisfies_clauses;
mod tuples;
mod type_paths;
mod types;
mod variable_declarations;

use super::*;
use crate::SyntaxLayer;
use galfus_core::{SourceFile, SourceId};

fn source(text: &str) -> SourceFile {
    SourceFile::new(SourceId::new(0), "test.gfs".to_string(), text.to_string())
}

fn find_first_of_kind(syntax: &SyntaxLayer, node: NodeId, kind: SyntaxNodeKind) -> Option<NodeId> {
    let syntax_node = syntax.node(node)?;

    if syntax_node.kind() == kind {
        return Some(node);
    }

    for child in syntax_node.children() {
        if let Some(found) = find_first_of_kind(syntax, *child, kind) {
            return Some(found);
        }
    }

    None
}
