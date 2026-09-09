use super::{GenericChoiceLayoutCache, GenericChoiceLayoutKey};
use galfus_contract::CapabilityCatalog;
use galfus_core::{DefId, ModuleId, ModulePath, Revision, SourceFile, SourceId, SymbolId};
use galfus_frontend::modules::{
    FrontendModuleKind, FrontendRoots, FrontendSession, FrontendSource, FrontendUpdate,
};
use std::collections::HashSet;
use std::sync::Arc;

fn path(value: &str) -> ModulePath {
    ModulePath::new(value).expect("test module path is valid")
}

#[test]
fn generic_choice_layout_cache_interns_an_instance_once_across_modules() {
    let mut cache = GenericChoiceLayoutCache::default();
    let key = GenericChoiceLayoutKey {
        def_id: DefId::new(ModuleId::new(7), SymbolId::new(3)),
        arguments: vec!["[u8]".to_string()],
    };

    let first = cache.intern(key.clone());
    let second = cache.intern(key);

    assert_eq!(first, second);
    assert_eq!(first.raw(), 0);
    assert_eq!(cache.len(), 1);
}

#[test]
fn generic_choice_layout_cache_discards_changed_modules_without_reusing_ids() {
    let mut cache = GenericChoiceLayoutCache::default();
    let changed_module = ModuleId::new(7);
    let retained_module = ModuleId::new(8);
    let changed_key = GenericChoiceLayoutKey {
        def_id: DefId::new(changed_module, SymbolId::new(3)),
        arguments: vec!["[u8]".to_string()],
    };
    let retained_key = GenericChoiceLayoutKey {
        def_id: DefId::new(retained_module, SymbolId::new(4)),
        arguments: vec!["i32".to_string()],
    };

    assert_eq!(cache.intern(changed_key), super::GlobalChoiceLayoutId(0));
    assert_eq!(
        cache.intern(retained_key.clone()),
        super::GlobalChoiceLayoutId(1)
    );

    cache.retain_modules(
        &HashSet::from([changed_module, retained_module]),
        &HashSet::from([changed_module]),
    );

    assert_eq!(cache.len(), 1);
    assert_eq!(cache.intern(retained_key), super::GlobalChoiceLayoutId(1));
    assert_eq!(
        cache.intern(GenericChoiceLayoutKey {
            def_id: DefId::new(changed_module, SymbolId::new(5)),
            arguments: vec!["bool".to_string()],
        }),
        super::GlobalChoiceLayoutId(2)
    );
}

#[test]
fn imported_choice_pattern_with_an_error_operand_is_reported() {
    let source = SourceFile::new(
        SourceId::new(1),
        "src/consumer.gfs".to_string(),
        "export fn read(): i32 { return 0 }".to_string(),
    );
    let sources = [FrontendSource {
        module_id: ModuleId::new(10),
        path: path("src/consumer.gfs"),
        source: &source,
        kind: FrontendModuleKind::Standard,
    }];
    let mut frontend = FrontendSession::new();
    let report = frontend.check(FrontendUpdate {
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
        catalog: Arc::new(CapabilityCatalog::default()),
    });
    assert!(
        !report.diagnostics.has_errors(),
        "{:#?}",
        report.diagnostics
    );

    let module = frontend.modules().first().expect("consumer module exists");
    let mut type_result = module
        .type_result()
        .cloned()
        .expect("consumer has type checking result");
    let error_type = type_result.layer_mut().table_mut().error();
    let mut choice_layouts = GenericChoiceLayoutCache::default();
    let mut ctx = super::LowerCtx::new(
        module.id(),
        &type_result,
        module.graph(),
        module.source().text(),
        &[],
        frontend.string_table(),
        module.path().as_str(),
        false,
        None,
        &mut choice_layouts,
    );

    let type_idx = super::types::lower_imported_choice_variant_type(
        &mut ctx,
        error_type,
        "StreamResult",
        "Data",
    );

    assert!(
        ctx.emission_errors.iter().any(
            |error| error.contains("cannot lower imported choice pattern `StreamResult::Data`")
        ),
        "{:#?}",
        ctx.emission_errors
    );
    assert!(matches!(
        ctx.types[type_idx.raw() as usize],
        galfus_bytecode::BytecodeType::Null
    ));
}
