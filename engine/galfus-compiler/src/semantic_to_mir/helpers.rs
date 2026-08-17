use super::function::FunctionBuilder;
use super::function_helpers::parse_int;
use galfus_core::{NodeId, SymbolId, TypeId};
use galfus_frontend::{PathReferenceKind, SymbolKind, SyntaxNodeKind};
use galfus_ir::mir::*;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn is_choice_variant_call_target(&self, target: NodeId) -> bool {
        let Some(resolution) = self.builder.graph.resolution() else {
            return false;
        };
        matches!(
            resolution.path_reference_kind(target),
            Some(PathReferenceKind::ChoiceVariant)
        )
    }

    pub(super) fn get_choice_variant_payload(
        &self,
        node: NodeId,
    ) -> Option<(String, TypeId, Vec<TypeId>)> {
        let resolution = self.builder.graph.resolution()?;
        let variant_symbol = resolution.path_reference_symbol(node)?;
        let owner_symbol = self.owner_symbol_for_member(variant_symbol, SymbolKind::Choice)?;

        let owner_type = self
            .builder
            .type_result
            .layer()
            .symbol_type(owner_symbol)
            .unwrap_or_else(|| TypeId::new(0));

        let variant_name = self
            .builder
            .string_table
            .resolve(resolution.symbol(variant_symbol)?.name())
            .unwrap_or("")
            .to_string();

        let payload_types = self.choice_variant_payload_types(owner_symbol, variant_symbol);

        Some((variant_name, owner_type, payload_types))
    }

    pub(super) fn owner_symbol_for_member(
        &self,
        member_symbol: SymbolId,
        owner_kind: SymbolKind,
    ) -> Option<SymbolId> {
        let resolution = self.builder.graph.resolution()?;
        for symbol in resolution.symbols() {
            if symbol.kind() != owner_kind {
                continue;
            }
            let has_member = resolution
                .member_scope(symbol.id())
                .and_then(|ms| resolution.scope(ms))
                .is_some_and(|scope| scope.symbols().values().any(|&sym| sym == member_symbol));
            if has_member {
                return Some(symbol.id());
            }
        }
        None
    }

    pub(super) fn choice_variant_payload_types(
        &self,
        owner_symbol: SymbolId,
        variant_symbol: SymbolId,
    ) -> Vec<TypeId> {
        let resolution = match self.builder.graph.resolution() {
            Some(res) => res,
            None => return Vec::new(),
        };
        let _owner_data = match resolution.symbol(owner_symbol) {
            Some(data) => data,
            None => return Vec::new(),
        };
        let variant_data = match resolution.symbol(variant_symbol) {
            Some(data) => data,
            None => return Vec::new(),
        };
        let root = self.builder.graph.syntax().root().unwrap();
        let choice_item = match self.choice_item_node_for_symbol(root, owner_symbol) {
            Some(node) => node,
            None => return Vec::new(),
        };
        let choice_node = match self.builder.graph.syntax().node(choice_item) {
            Some(node) => node,
            None => return Vec::new(),
        };
        let mut variant_node = None;
        for &child in choice_node.children() {
            if let Some(node) = self.find_choice_variant_node_by_name(
                child,
                self.builder
                    .string_table
                    .resolve(variant_data.name())
                    .unwrap_or(""),
            ) {
                variant_node = Some(node);
                break;
            }
        }
        let variant_node_id = match variant_node {
            Some(id) => id,
            None => return Vec::new(),
        };
        let payload = match self
            .builder
            .find_descendant_of_kind(variant_node_id, SyntaxNodeKind::ChoicePayload)
        {
            Some(id) => id,
            None => return Vec::new(),
        };
        let payload_node = match self.builder.graph.syntax().node(payload) {
            Some(node) => node,
            None => return Vec::new(),
        };

        payload_node
            .children()
            .iter()
            .filter_map(|child| {
                let type_node = self.first_type_child(*child).unwrap_or(*child);
                self.builder.type_result.layer().node_type(type_node)
            })
            .collect()
    }

    pub(super) fn choice_item_node_for_symbol(
        &self,
        node: NodeId,
        choice_symbol: SymbolId,
    ) -> Option<NodeId> {
        let syntax_node = self.builder.graph.syntax().node(node)?;
        if syntax_node.kind() == SyntaxNodeKind::ChoiceItem {
            let matches_symbol = self.builder.graph.resolution().is_some_and(|res| {
                self.builder
                    .graph
                    .syntax()
                    .first_child_of_kind(node, SyntaxNodeKind::Identifier)
                    .and_then(|ident| res.declaration_symbol(ident))
                    == Some(choice_symbol)
            });
            if matches_symbol {
                return Some(node);
            }
        }
        for &child in syntax_node.children() {
            if let Some(found) = self.choice_item_node_for_symbol(child, choice_symbol) {
                return Some(found);
            }
        }
        None
    }

    pub(super) fn find_choice_variant_node_by_name(
        &self,
        node: NodeId,
        variant_name: &str,
    ) -> Option<NodeId> {
        let syntax = self.builder.graph.syntax();
        let syntax_node = syntax.node(node)?;
        if syntax_node.kind() == SyntaxNodeKind::ChoiceVariant {
            let matches_name = syntax
                .first_child_of_kind(node, SyntaxNodeKind::Identifier)
                .is_some_and(|ident| self.builder.node_text(ident) == variant_name);
            if matches_name {
                return Some(node);
            }
        }
        for &child in syntax_node.children() {
            if let Some(found) = self.find_choice_variant_node_by_name(child, variant_name) {
                return Some(found);
            }
        }
        None
    }

    pub(super) fn first_type_child(&self, node: NodeId) -> Option<NodeId> {
        let syntax = self.builder.graph.syntax();
        let syntax_node = syntax.node(node)?;
        for &child in syntax_node.children() {
            let is_type = syntax
                .node(child)
                .is_some_and(|child_node| self.is_type_node_kind(child_node.kind()));
            if is_type {
                return Some(child);
            }
            if let Some(found) = self.first_type_child(child) {
                return Some(found);
            }
        }
        None
    }

    pub(super) fn is_type_node_kind(&self, kind: SyntaxNodeKind) -> bool {
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

    pub(super) fn resolve_alias_type(&self, ty: TypeId) -> TypeId {
        self.builder.resolve_alias_type(ty)
    }

    pub(super) fn get_enum_variant_value(&self, variant_symbol: SymbolId) -> i64 {
        let resolution = match self.builder.graph.resolution() {
            Some(res) => res,
            None => return 0,
        };
        let enum_symbol = match self.owner_symbol_for_member(variant_symbol, SymbolKind::Enum) {
            Some(sym) => sym,
            None => return 0,
        };
        let root = self.builder.graph.syntax().root().unwrap();
        let enum_item = match self.find_enum_item_node_for_symbol(root, enum_symbol) {
            Some(node) => node,
            None => return 0,
        };
        let mut variants = Vec::new();
        self.collect_enum_variants(enum_item, &mut variants);

        let mut current_value = 0;
        for &variant_node in &variants {
            if let Some(ident) = self
                .builder
                .graph
                .syntax()
                .first_child_of_kind(variant_node, SyntaxNodeKind::Identifier)
            {
                let symbol = resolution.declaration_symbol(ident);
                if let Some(val_node) = self
                    .builder
                    .graph
                    .syntax()
                    .first_child_of_kind(variant_node, SyntaxNodeKind::EnumDiscriminant)
                    .and_then(|discriminant| self.builder.graph.syntax().child(discriminant, 0))
                {
                    let text = self.builder.node_text(val_node);
                    current_value = parse_int(text).unwrap_or(current_value);
                }
                if symbol == Some(variant_symbol) {
                    return current_value;
                }
            }
            current_value += 1;
        }
        0
    }

    pub(super) fn find_enum_item_node_for_symbol(
        &self,
        node: NodeId,
        enum_symbol: SymbolId,
    ) -> Option<NodeId> {
        let syntax_node = self.builder.graph.syntax().node(node)?;
        if syntax_node.kind() == SyntaxNodeKind::EnumItem {
            let matches_symbol = self.builder.graph.resolution().is_some_and(|res| {
                self.builder
                    .graph
                    .syntax()
                    .first_child_of_kind(node, SyntaxNodeKind::Identifier)
                    .and_then(|ident| res.declaration_symbol(ident))
                    == Some(enum_symbol)
            });
            if matches_symbol {
                return Some(node);
            }
        }
        for &child in syntax_node.children() {
            if let Some(found) = self.find_enum_item_node_for_symbol(child, enum_symbol) {
                return Some(found);
            }
        }
        None
    }

    pub(super) fn collect_enum_variants(&self, node: NodeId, variants: &mut Vec<NodeId>) {
        let syntax_node = match self.builder.graph.syntax().node(node) {
            Some(n) => n,
            None => return,
        };
        if syntax_node.kind() == SyntaxNodeKind::EnumVariant {
            variants.push(node);
            return;
        }
        for &child in syntax_node.children() {
            self.collect_enum_variants(child, variants);
        }
    }

    pub(super) fn declaration_symbols_in_node(
        &self,
        node: NodeId,
        kinds: &[SymbolKind],
    ) -> Vec<SymbolId> {
        let mut symbols = self.collect_declaration_symbols(node);
        if let Some(res) = self.builder.graph.resolution() {
            symbols.retain(|&symbol| {
                if let Some(sym_data) = res.symbol(symbol) {
                    kinds.contains(&sym_data.kind())
                } else {
                    false
                }
            });
        }
        symbols
    }

    pub(super) fn insert_cast_if_needed(
        &mut self,
        operand: Operand,
        from_ty: TypeId,
        to_ty: TypeId,
    ) -> Operand {
        let from_ty = self.builder.resolve_alias_type(from_ty);
        let to_ty = self.builder.resolve_alias_type(to_ty);

        let actual_from_ty = match &operand {
            Operand::Constant(Constant::Int8(_)) => self
                .builder
                .type_result
                .layer()
                .table()
                .primitive(galfus_frontend::PrimitiveType::Int8),
            Operand::Constant(Constant::Int16(_)) => self
                .builder
                .type_result
                .layer()
                .table()
                .primitive(galfus_frontend::PrimitiveType::Int16),
            Operand::Constant(Constant::Int32(_)) => self
                .builder
                .type_result
                .layer()
                .table()
                .primitive(galfus_frontend::PrimitiveType::Int32),
            Operand::Constant(Constant::Int64(_)) => self
                .builder
                .type_result
                .layer()
                .table()
                .primitive(galfus_frontend::PrimitiveType::Int64),
            Operand::Constant(Constant::Uint8(_)) => self
                .builder
                .type_result
                .layer()
                .table()
                .primitive(galfus_frontend::PrimitiveType::Uint8),
            Operand::Constant(Constant::Uint16(_)) => self
                .builder
                .type_result
                .layer()
                .table()
                .primitive(galfus_frontend::PrimitiveType::Uint16),
            Operand::Constant(Constant::Uint32(_)) => self
                .builder
                .type_result
                .layer()
                .table()
                .primitive(galfus_frontend::PrimitiveType::Uint32),
            Operand::Constant(Constant::Uint64(_)) => self
                .builder
                .type_result
                .layer()
                .table()
                .primitive(galfus_frontend::PrimitiveType::Uint64),
            Operand::Constant(Constant::Float32(_)) => self
                .builder
                .type_result
                .layer()
                .table()
                .primitive(galfus_frontend::PrimitiveType::Float32),
            Operand::Constant(Constant::Float64(_)) => self
                .builder
                .type_result
                .layer()
                .table()
                .primitive(galfus_frontend::PrimitiveType::Float64),
            _ => from_ty,
        };

        if actual_from_ty == to_ty {
            return operand;
        }

        let table = self.builder.type_result.layer().table();
        let is_numeric = |ty: TypeId| {
            matches!(
                table.kind(ty),
                Some(galfus_frontend::TypeKind::Primitive(
                    galfus_frontend::PrimitiveType::Int8
                        | galfus_frontend::PrimitiveType::Int16
                        | galfus_frontend::PrimitiveType::Int32
                        | galfus_frontend::PrimitiveType::Int64
                        | galfus_frontend::PrimitiveType::Uint8
                        | galfus_frontend::PrimitiveType::Uint16
                        | galfus_frontend::PrimitiveType::Uint32
                        | galfus_frontend::PrimitiveType::Uint64
                        | galfus_frontend::PrimitiveType::Float32
                        | galfus_frontend::PrimitiveType::Float64
                ))
            )
        };

        if is_numeric(actual_from_ty) && is_numeric(to_ty) {
            let temp_id = self.declare_local(None, to_ty);
            self.current_instructions.push((
                Instruction::Assign(temp_id, RValue::Cast(operand, to_ty)),
                None,
            ));
            Operand::Local(temp_id)
        } else {
            operand
        }
    }
}
