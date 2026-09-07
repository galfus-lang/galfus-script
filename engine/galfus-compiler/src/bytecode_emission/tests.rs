use super::{GenericChoiceLayoutCache, GenericChoiceLayoutKey};
use galfus_core::{DefId, ModuleId, SymbolId};

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
