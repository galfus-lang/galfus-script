use crate::{
    FunctionParameterType, ModuleAst, PrimitiveType, SymbolKind, SyntaxNodeKind, TypeLayer,
};
use galfus_core::{NodeId, SourceFile, SymbolId, TypeId};

#[derive(Debug, Clone)]
pub struct TypeBindResult {
    layer: TypeLayer,
}

impl TypeBindResult {
    pub fn new(layer: TypeLayer) -> Self {
        Self { layer }
    }

    pub fn layer(&self) -> &TypeLayer {
        &self.layer
    }

    pub fn into_layer(self) -> TypeLayer {
        self.layer
    }
}

pub type TypeLoweringResult = TypeBindResult;

pub fn bind_types(source: &SourceFile, graph: &ModuleAst) -> TypeBindResult {
    let mut binder = TypeBinder::new(source, graph);
    binder.bind();

    TypeBindResult::new(binder.into_layer())
}

pub fn lower_types(source: &SourceFile, graph: &ModuleAst) -> TypeLoweringResult {
    bind_types(source, graph)
}

struct TypeBinder<'a> {
    source: &'a SourceFile,
    graph: &'a ModuleAst,
    layer: TypeLayer,
}

impl<'a> TypeBinder<'a> {
    fn new(source: &'a SourceFile, graph: &'a ModuleAst) -> Self {
        Self {
            source,
            graph,
            layer: TypeLayer::new(),
        }
    }

    fn into_layer(self) -> TypeLayer {
        self.layer
    }

    fn bind(&mut self) {
        let Some(root) = self.graph.syntax().root() else {
            return;
        };

        self.lower_types_in_node(root);
    }

    fn lower_types_in_node(&mut self, node: NodeId) {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return;
        };

        if self.is_type_node_kind(syntax_node.kind()) {
            let ty = self.lower_type(node);
            self.layer.bind_node_type(node, ty);
        }

        for child in syntax_node.children() {
            self.lower_types_in_node(*child);
        }
    }

    fn lower_type(&mut self, node: NodeId) -> TypeId {
        let Some(syntax_node) = self.graph.syntax().node(node) else {
            return self.error_type();
        };

        match syntax_node.kind() {
            SyntaxNodeKind::TypeNull => self.layer.table().primitive(PrimitiveType::Null),

            SyntaxNodeKind::NamedType => self.lower_named_type(node),

            SyntaxNodeKind::Path => self.lower_path_type(node),

            SyntaxNodeKind::ArrayType => self.lower_array_type(node),

            SyntaxNodeKind::TupleType => self.lower_tuple_type(node),

            SyntaxNodeKind::GroupedType => self.lower_grouped_type(node),

            SyntaxNodeKind::UnionType => self.lower_union_type(node),

            SyntaxNodeKind::GenericType => self.lower_generic_type(node),

            SyntaxNodeKind::FunctionType => self.lower_function_type(node),

            _ => self.error_type(),
        }
    }

    fn lower_named_type(&mut self, node: NodeId) -> TypeId {
        let Some(identifier) = self
            .graph
            .syntax()
            .first_child_of_kind(node, SyntaxNodeKind::Identifier)
        else {
            return self.error_type();
        };

        let name = self.node_text(identifier);

        if let Some(family) = self.layer.table_mut().primitive_family(name.as_str()) {
            return family;
        }

        if let Some(primitive) = primitive_type_by_name(name.as_str()) {
            return self.layer.table().primitive(primitive);
        }

        let Some(resolution) = self.graph.resolution() else {
            return self.error_type();
        };

        let Some(symbol) = resolution.type_reference_symbol(node) else {
            return self.error_type();
        };

        self.lower_symbol_type(symbol)
    }

    fn lower_path_type(&mut self, node: NodeId) -> TypeId {
        let Some(resolution) = self.graph.resolution() else {
            return self.error_type();
        };

        if let Some(symbol) = resolution.type_path_reference_symbol(node) {
            return self.lower_symbol_type(symbol);
        }

        let Some(root_symbol) = resolution.type_reference_symbol(node) else {
            return self.error_type();
        };

        let Some(symbol_data) = resolution.symbol(root_symbol) else {
            return self.error_type();
        };

        if symbol_data.kind() == SymbolKind::ImportNamespace {
            let segments = self.path_segments(node);

            if segments.len() <= 1 {
                return self.error_type();
            }

            return self
                .layer
                .table_mut()
                .intern_path(root_symbol, segments.into_iter().skip(1).collect());
        }

        self.lower_symbol_type(root_symbol)
    }

    fn lower_symbol_type(&mut self, symbol: SymbolId) -> TypeId {
        let Some(resolution) = self.graph.resolution() else {
            return self.error_type();
        };

        let Some(symbol_data) = resolution.symbol(symbol) else {
            return self.error_type();
        };

        if symbol_data.kind() == SymbolKind::BuiltinType {
            if let Some(family) = self.layer.table_mut().primitive_family(symbol_data.name()) {
                return family;
            }
            if let Some(primitive) = primitive_type_by_name(symbol_data.name()) {
                return self.layer.table().primitive(primitive);
            }

            return self.error_type();
        }

        if symbol_data.kind() == SymbolKind::GenericParameter {
            return self.layer.table_mut().intern_generic_parameter(symbol);
        }

        self.layer.table_mut().intern_named(symbol)
    }

    fn lower_array_type(&mut self, node: NodeId) -> TypeId {
        let Some(element) = self.graph.syntax().child(node, 0) else {
            return self.error_type();
        };

        let element = self.lower_type(element);
        self.layer.table_mut().intern_array(element)
    }

    fn lower_tuple_type(&mut self, node: NodeId) -> TypeId {
        let elements = self
            .graph
            .syntax()
            .node(node)
            .map(|node| node.children().to_vec())
            .unwrap_or_default()
            .into_iter()
            .map(|child| self.lower_type(child))
            .collect::<Vec<_>>();

        self.layer.table_mut().intern_tuple(elements)
    }

    fn lower_grouped_type(&mut self, node: NodeId) -> TypeId {
        let Some(inner) = self.graph.syntax().child(node, 0) else {
            return self.error_type();
        };

        self.lower_type(inner)
    }

    fn lower_union_type(&mut self, node: NodeId) -> TypeId {
        let members = self
            .graph
            .syntax()
            .node(node)
            .map(|node| node.children().to_vec())
            .unwrap_or_default()
            .into_iter()
            .map(|child| self.lower_type(child))
            .collect::<Vec<_>>();

        self.layer.table_mut().intern_union(members)
    }

    fn lower_generic_type(&mut self, node: NodeId) -> TypeId {
        let Some(base) = self.graph.syntax().child(node, 0) else {
            return self.error_type();
        };

        let Some(arguments) = self.graph.syntax().child(node, 1) else {
            return self.error_type();
        };

        let base = self.lower_type(base);

        let arguments = self
            .graph
            .syntax()
            .node(arguments)
            .map(|node| node.children().to_vec())
            .unwrap_or_default()
            .into_iter()
            .map(|argument| self.lower_type(argument))
            .collect::<Vec<_>>();

        self.layer
            .table_mut()
            .intern_generic_instance(base, arguments)
    }

    fn lower_function_type(&mut self, node: NodeId) -> TypeId {
        let Some(parameters_node) = self.graph.syntax().child(node, 0) else {
            return self.error_type();
        };

        let Some(return_type_node) = self.graph.syntax().child(node, 1) else {
            return self.error_type();
        };

        let parameters = self
            .graph
            .syntax()
            .node(parameters_node)
            .map(|node| node.children().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|parameter| self.lower_function_type_parameter(parameter))
            .collect::<Vec<_>>();

        let return_type = self.lower_type(return_type_node);

        self.layer
            .table_mut()
            .intern_function(parameters, return_type)
    }

    fn lower_function_type_parameter(
        &mut self,
        parameter: NodeId,
    ) -> Option<FunctionParameterType> {
        let type_node = if self.is_type_node(parameter) {
            parameter
        } else {
            self.first_type_child(parameter)?
        };

        Some(FunctionParameterType::new(self.lower_type(type_node)))
    }

    fn first_type_child(&self, node: NodeId) -> Option<NodeId> {
        let syntax_node = self.graph.syntax().node(node)?;

        for child in syntax_node.children() {
            if self.is_type_node(*child) {
                return Some(*child);
            }

            if let Some(found) = self.first_type_child(*child) {
                return Some(found);
            }
        }

        None
    }

    fn is_type_node(&self, node: NodeId) -> bool {
        self.graph
            .syntax()
            .node(node)
            .map(|node| self.is_type_node_kind(node.kind()))
            .unwrap_or(false)
    }

    fn is_type_node_kind(&self, kind: SyntaxNodeKind) -> bool {
        matches!(
            kind,
            SyntaxNodeKind::TypeNull
                | SyntaxNodeKind::NamedType
                | SyntaxNodeKind::Path
                | SyntaxNodeKind::ArrayType
                | SyntaxNodeKind::TupleType
                | SyntaxNodeKind::GroupedType
                | SyntaxNodeKind::UnionType
                | SyntaxNodeKind::GenericType
                | SyntaxNodeKind::FunctionType
        )
    }

    fn path_segments(&self, path: NodeId) -> Vec<String> {
        let Some(path_node) = self.graph.syntax().node(path) else {
            return Vec::new();
        };

        path_node
            .children()
            .iter()
            .filter_map(|child| {
                let child_node = self.graph.syntax().node(*child)?;

                if child_node.kind() != SyntaxNodeKind::Identifier {
                    return None;
                }

                Some(self.node_text(*child))
            })
            .collect()
    }

    fn node_text(&self, node: NodeId) -> String {
        let Some(node) = self.graph.syntax().node(node) else {
            return String::new();
        };

        self.source.slice(node.span()).unwrap_or("").to_string()
    }

    fn error_type(&mut self) -> TypeId {
        self.layer.table_mut().error()
    }
}

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
