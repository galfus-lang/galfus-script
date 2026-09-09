use super::function::FunctionBuilder;
use galfus_core::{FunctionId, NodeId, SymbolId, TypeId};
use galfus_frontend::{SymbolKind, SyntaxNodeKind, TypeKind};

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn call_target_symbol(&self, target: NodeId) -> Option<SymbolId> {
        let syntax = self.builder.graph.syntax();
        let resolution = self.builder.graph.resolution()?;
        let node = syntax.node(target)?;

        match node.kind() {
            SyntaxNodeKind::NameExpression => resolution.reference_symbol(target).or_else(|| {
                let ident = syntax.first_child_of_kind(target, SyntaxNodeKind::Identifier)?;
                resolution.reference_symbol(ident)
            }),
            SyntaxNodeKind::PathExpression => resolution
                .path_reference_symbol(target)
                .or_else(|| resolution.reference_symbol(target)),
            SyntaxNodeKind::GenericExpression => syntax
                .child(target, 0)
                .and_then(|inner| self.call_target_symbol(inner)),
            _ => None,
        }
    }

    pub(super) fn typeof_subject_type(&self, subject: NodeId) -> Option<TypeId> {
        let syntax = self.builder.graph.syntax();
        let resolution = self.builder.graph.resolution()?;

        let generic_parameter_type = resolution
            .reference_symbol(subject)
            .or_else(|| {
                syntax
                    .first_child_of_kind(subject, SyntaxNodeKind::Identifier)
                    .and_then(|identifier| resolution.reference_symbol(identifier))
            })
            .and_then(|symbol| self.builder.type_result.layer().symbol_type(symbol))
            .filter(|ty| {
                matches!(
                    self.builder.type_result.layer().table().kind(*ty),
                    Some(TypeKind::GenericParameter { .. })
                )
            })
            .map(|ty| self.substitute_type(ty));

        generic_parameter_type.or_else(|| self.node_type(subject))
    }

    pub(super) fn function_id_for_symbol(&self, symbol: SymbolId, target: NodeId) -> FunctionId {
        let is_import = self
            .builder
            .graph
            .resolution()
            .and_then(|resolution| resolution.import_for_symbol(symbol))
            .is_some();

        if is_import {
            path_call_function_id(target)
        } else {
            FunctionId::new(symbol.raw())
        }
    }

    pub(super) fn anchored_call_receiver(&self, target: NodeId) -> Option<NodeId> {
        if self.is_choice_variant_call_target(target) {
            return None;
        }

        let syntax = self.builder.graph.syntax();
        let node = syntax.node(target)?;
        if node.kind() != SyntaxNodeKind::PathExpression {
            return None;
        }

        let receiver = node.child(0)?;
        let receiver_kind = syntax.node(receiver)?.kind();
        if matches!(
            receiver_kind,
            SyntaxNodeKind::Identifier | SyntaxNodeKind::Path | SyntaxNodeKind::GenericExpression
        ) {
            None
        } else if receiver_kind == SyntaxNodeKind::NameExpression {
            if let Some(resolution) = self.builder.graph.resolution()
                && let Some(symbol) = resolution.reference_symbol(receiver)
                && let Some(symbol_data) = resolution.symbol(symbol)
                && matches!(
                    symbol_data.kind(),
                    SymbolKind::ImportBinding | SymbolKind::ImportNamespace | SymbolKind::Struct
                )
            {
                None
            } else {
                Some(receiver)
            }
        } else {
            Some(receiver)
        }
    }

    pub(super) fn anchored_function_symbol(
        &self,
        receiver: NodeId,
        target: NodeId,
    ) -> Option<SymbolId> {
        let syntax = self.builder.graph.syntax();
        let resolution = self.builder.graph.resolution()?;
        let member = syntax.child(target, 1)?;
        let member_name = self.builder.node_text(member);

        let receiver_ty = self.node_type(receiver)?;
        let receiver_ty = self.builder.resolve_alias_type(receiver_ty);
        let TypeKind::Named { symbol } =
            self.builder.type_result.layer().table().kind(receiver_ty)?
        else {
            return None;
        };

        let receiver_symbol = resolution.symbol(*symbol)?;
        if receiver_symbol.kind() != SymbolKind::Struct {
            return None;
        }

        let receiver_name = self.builder.string_table.resolve(receiver_symbol.name())?;
        let function_name = format!("{receiver_name}::{member_name}");
        resolution
            .symbols()
            .iter()
            .find(|symbol| {
                symbol.kind() == SymbolKind::Function
                    && self
                        .builder
                        .string_table
                        .resolve(symbol.name())
                        .unwrap_or("")
                        == function_name.as_str()
            })
            .map(|symbol| symbol.id())
    }

    /// Imported struct methods are resolved through an import slot during
    /// bytecode compilation, so there is no local method symbol to return.
    pub(super) fn is_imported_struct_receiver(&self, receiver: NodeId) -> bool {
        let Some(receiver_ty) = self.node_type(receiver) else {
            return false;
        };
        let receiver_ty = self.builder.resolve_alias_type(receiver_ty);
        match self.builder.type_result.layer().table().kind(receiver_ty) {
            Some(TypeKind::Named { symbol }) => {
                self.builder.graph.resolution().is_some_and(|resolution| {
                    let is_local = resolution.symbols().iter().any(|item| item.id() == *symbol);
                    resolution.import_for_symbol(*symbol).is_some() || !is_local
                })
            }
            Some(TypeKind::Path { .. }) => true,
            _ => false,
        }
    }
}

const PATH_CALL_TARGET_TAG: u32 = 0x8000_0000;

pub(super) fn path_call_function_id(node: NodeId) -> FunctionId {
    FunctionId::new(PATH_CALL_TARGET_TAG | node.raw())
}
