use super::function::FunctionBuilder;
use galfus_core::TypeId;
use galfus_frontend::TypeKind;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn is_future_type(&self, type_id: TypeId) -> bool {
        let table = self.builder.type_result.layer().table();
        let Some(TypeKind::GenericInstance { base, .. }) =
            table.kind(self.builder.resolve_alias_type(type_id))
        else {
            return false;
        };
        match table.kind(*base) {
            Some(TypeKind::Named { symbol }) => self
                .builder
                .graph
                .resolution()
                .and_then(|resolution| resolution.symbol(*symbol))
                .is_some_and(|symbol| {
                    self.builder.string_table.resolve(symbol.name()) == Some("Future")
                }),
            Some(TypeKind::Path { segments, .. }) => {
                segments.last().is_some_and(|segment| segment == "Future")
            }
            _ => false,
        }
    }
}
