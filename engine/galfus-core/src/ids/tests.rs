use super::*;

fn assert_serializable<T: serde::Serialize + for<'de> serde::Deserialize<'de>>() {}

#[test]
fn def_id_is_serializable_and_preserves_its_local_identity() {
    assert_serializable::<DefId>();

    let def_id = DefId::new(ModuleId::new(7), SymbolId::new(11));
    assert_eq!(def_id.module, ModuleId::new(7));
    assert_eq!(def_id.local, SymbolId::new(11));
    assert_eq!(DefId::local(SymbolId::new(11)).module, ModuleId::new(0));
}
