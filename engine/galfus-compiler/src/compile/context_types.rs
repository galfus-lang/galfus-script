use super::context::MyWorkspaceContext;
use galfus_core::{SymbolId, TypeId};
use galfus_frontend::{FunctionParameterType, FunctionType, ResolutionLayer, TypeKind, TypeTable};

impl<'a> MyWorkspaceContext<'a> {
    fn translate_symbol(
        string_table: &galfus_frontend::StringTable,
        caller_resolution: Option<&ResolutionLayer>,
        target_resolution: Option<&ResolutionLayer>,
        symbol: SymbolId,
    ) -> SymbolId {
        let Some(caller_resolution) = caller_resolution else {
            return symbol;
        };
        let Some(caller_symbol) = caller_resolution.symbol(symbol) else {
            return symbol;
        };
        let name = string_table.resolve(caller_symbol.name()).unwrap_or("");

        let Some(target_resolution) = target_resolution else {
            return symbol;
        };
        if let Some(target_symbol) = target_resolution
            .symbols()
            .iter()
            .find(|target_symbol| string_table.resolve(target_symbol.name()).unwrap_or("") == name)
        {
            return target_symbol.id();
        }
        target_resolution
            .imports()
            .iter()
            .find(|import| import.local_name() == name)
            .map(|import| import.local_symbol())
            .unwrap_or(symbol)
    }

    pub(super) fn translate_type(
        &mut self,
        caller_mod_idx: usize,
        target_mod_idx: usize,
        ty: TypeId,
    ) -> TypeId {
        if caller_mod_idx == target_mod_idx {
            return ty;
        }

        let (caller_module, target_module) = if caller_mod_idx < target_mod_idx {
            let (before_target, after_target) = self.modules.split_at_mut(target_mod_idx);
            (&before_target[caller_mod_idx], &mut after_target[0])
        } else {
            let (before_caller, after_caller) = self.modules.split_at_mut(caller_mod_idx);
            (&after_caller[0], &mut before_caller[target_mod_idx])
        };
        let caller_table = caller_module.type_result.as_ref().unwrap().layer().table();
        let caller_resolution = caller_module.graph.resolution();
        let target_resolution = target_module.graph.resolution();
        let target_table = target_module
            .type_result
            .as_mut()
            .unwrap()
            .layer_mut()
            .table_mut();

        Self::translate_type_helper(
            self.string_table,
            caller_resolution,
            target_resolution,
            caller_table,
            target_table,
            ty,
        )
    }

    fn translate_type_helper(
        string_table: &galfus_frontend::StringTable,
        caller_resolution: Option<&ResolutionLayer>,
        target_resolution: Option<&ResolutionLayer>,
        caller_table: &TypeTable,
        target_table: &mut TypeTable,
        ty: TypeId,
    ) -> TypeId {
        let Some(kind) = caller_table.kind(ty) else {
            return ty;
        };

        let mut translate = |ty| {
            Self::translate_type_helper(
                string_table,
                caller_resolution,
                target_resolution,
                caller_table,
                target_table,
                ty,
            )
        };
        let translated_kind = match kind {
            TypeKind::Primitive(primitive) => TypeKind::Primitive(*primitive),
            TypeKind::Named { symbol } => TypeKind::Named {
                symbol: Self::translate_symbol(
                    string_table,
                    caller_resolution,
                    target_resolution,
                    *symbol,
                ),
            },
            TypeKind::GenericParameter { symbol } => TypeKind::GenericParameter {
                symbol: Self::translate_symbol(
                    string_table,
                    caller_resolution,
                    target_resolution,
                    *symbol,
                ),
            },
            TypeKind::Array { element } => TypeKind::Array {
                element: translate(*element),
            },
            TypeKind::Range { element } => TypeKind::Range {
                element: translate(*element),
            },
            TypeKind::Tuple { elements } => TypeKind::Tuple {
                elements: elements.iter().map(|element| translate(*element)).collect(),
            },
            TypeKind::Union { members } => TypeKind::Union {
                members: members.iter().map(|member| translate(*member)).collect(),
            },
            TypeKind::Function(function) => {
                let parameters = function
                    .parameters()
                    .iter()
                    .map(|parameter| {
                        let ty = translate(parameter.ty());
                        if parameter.is_rest() {
                            FunctionParameterType::rest(ty)
                        } else if parameter.has_default() {
                            FunctionParameterType::with_default(ty, None)
                        } else {
                            FunctionParameterType::new(ty)
                        }
                    })
                    .collect();
                TypeKind::Function(FunctionType::new(
                    parameters,
                    translate(function.return_type()),
                    function.is_external(),
                ))
            }
            TypeKind::GenericInstance { base, arguments } => TypeKind::GenericInstance {
                base: translate(*base),
                arguments: arguments
                    .iter()
                    .map(|argument| translate(*argument))
                    .collect(),
            },
            TypeKind::Path { root, segments } => TypeKind::Path {
                root: Self::translate_symbol(
                    string_table,
                    caller_resolution,
                    target_resolution,
                    *root,
                ),
                segments: segments.clone(),
            },
            TypeKind::Error => TypeKind::Error,
        };
        target_table.intern(translated_kind)
    }
}
