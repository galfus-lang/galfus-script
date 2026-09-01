use galfus_core::{ModuleId, NodeId, SymbolId};
use galfus_frontend::modules::FrontendSnapshot;
use galfus_frontend::{SymbolKind, TypeKind};

pub(crate) fn value_anchor_function(
    snapshot: &FrontendSnapshot,
    module: &galfus_frontend::modules::SemanticModule,
    path: NodeId,
) -> Option<(ModuleId, SymbolId)> {
    let graph = module.graph();
    let syntax = graph.syntax();
    let resolution = graph.resolution()?;
    let type_result = module.type_result()?;

    if resolution.path_reference_symbol(path).is_some() {
        return None;
    }

    let target = syntax.child(path, 0)?;
    let member = syntax.child(path, 1)?;
    let target_type = type_result.layer().node_type(target).or_else(|| {
        resolution
            .reference_symbol(target)
            .and_then(|symbol| type_result.layer().symbol_type(symbol))
    })?;
    let table = type_result.layer().table();
    let owner_symbol = match table.kind(target_type)? {
        TypeKind::Named { symbol } => *symbol,
        TypeKind::GenericInstance { base, .. } => {
            let TypeKind::Named { symbol } = table.kind(*base)? else {
                return None;
            };
            *symbol
        }
        _ => return None,
    };
    let (owner_module_id, owner_symbol, owner_resolution) = if resolution
        .symbol(owner_symbol)
        .is_some_and(|symbol| symbol.kind() == SymbolKind::ImportBinding)
    {
        let import = resolution.import(resolution.import_for_symbol(owner_symbol)?)?;
        let edge = snapshot
            .semantic_graph()
            .import_edges()
            .iter()
            .find(|edge| {
                edge.from() == module.id()
                    && edge.source() == import.source()
                    && edge.imported_name() == import.imported_name()
            })?;
        let target_module = snapshot.semantic_graph().get(edge.to()?)?;
        let target_resolution = target_module.graph().resolution()?;
        let export = target_resolution
            .export_record(target_resolution.export_by_name(edge.export_name()?)?)?;
        (target_module.id(), export.symbol(), target_resolution)
    } else {
        (module.id(), owner_symbol, resolution)
    };

    let owner = owner_resolution.symbol(owner_symbol)?;
    if owner.kind() != SymbolKind::Struct {
        return None;
    }

    let string_table = snapshot.string_table();
    let owner_name = string_table.resolve(owner.name())?;
    let member_name = module.source().slice(syntax.node(member)?.span())?;
    let anchored_name = format!("{owner_name}::{member_name}");
    let name = string_table.get(anchored_name.as_str())?;
    let symbol = owner_resolution
        .scope(owner_resolution.module_scope())?
        .symbol(name)?;

    owner_resolution
        .symbol(symbol)
        .filter(|symbol| symbol.kind() == SymbolKind::Function)
        .filter(|symbol| {
            owner_module_id == module.id()
                || owner_resolution.export_for_symbol(symbol.id()).is_some()
        })
        .map(|symbol| (owner_module_id, symbol.id()))
}

pub(crate) fn enclosing_path_expression(
    syntax: &galfus_frontend::SyntaxLayer,
    path_stack: &[NodeId],
) -> Option<NodeId> {
    path_stack.iter().rev().copied().find(|node| {
        syntax
            .node(*node)
            .is_some_and(|node| node.kind() == galfus_frontend::SyntaxNodeKind::PathExpression)
    })
}
