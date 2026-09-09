use super::function::FunctionBuilder;
use galfus_core::{NodeId, SymbolId, TypeId};
use galfus_frontend::TypeKind;

impl<'b, 'a> FunctionBuilder<'b, 'a> {
    pub(super) fn variant_pattern_symbols(&self, pattern: NodeId) -> Option<(SymbolId, SymbolId)> {
        let resolution = self.builder.graph.resolution()?;
        Some((
            resolution.reference_symbol(pattern)?,
            resolution.path_reference_symbol(pattern)?,
        ))
    }

    pub(super) fn get_imported_choice_variant(
        &self,
        pattern: NodeId,
    ) -> Option<(String, String, Vec<TypeId>)> {
        let resolution = self.builder.graph.resolution()?;
        let owner_symbol = resolution.reference_symbol(pattern).or_else(|| {
            self.builder
                .graph
                .syntax()
                .child(pattern, 0)
                .and_then(|root| resolution.reference_symbol(root))
        });
        if let Some(owner_symbol) = owner_symbol
            && let Some(choice) = self
                .builder
                .type_result
                .imported_symbol_choices
                .get(&owner_symbol)
        {
            let variant_name = self
                .builder
                .graph
                .syntax()
                .child(pattern, 1)
                .map(|node| self.builder.node_text(node))?;
            let variant = choice
                .variants
                .iter()
                .find(|variant| variant.name == variant_name)?;
            return Some((
                choice.name.clone(),
                variant.name.clone(),
                variant.payload_types.clone(),
            ));
        }

        let variant_type = self.builder.type_result.layer().node_type(pattern)?;
        let table = self.builder.type_result.layer().table();
        let base_type = match table.kind(variant_type) {
            Some(TypeKind::GenericInstance { base, .. }) => *base,
            _ => variant_type,
        };
        let TypeKind::Path { segments, .. } = table.kind(base_type)? else {
            return None;
        };
        let variant_name = segments.last()?;
        let choice = self.imported_choice_for_type(base_type)?;
        let variant = choice
            .variants
            .iter()
            .find(|variant| variant.name == *variant_name)?;
        Some((
            choice.name.clone(),
            variant.name.clone(),
            variant.payload_types.clone(),
        ))
    }
}
