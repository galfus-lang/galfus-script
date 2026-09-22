use super::{
    RuntimeExportId, RuntimeExportIdCollision, RuntimeExportIdRegistry, RuntimeExportIdentity,
    RuntimeExportKind,
};
use crate::ModuleId;

fn assert_serializable<T: serde::Serialize + for<'de> serde::Deserialize<'de>>() {}

#[test]
fn runtime_export_id_is_stable_and_distinguishes_export_identity() {
    assert_serializable::<RuntimeExportId>();
    assert_serializable::<RuntimeExportKind>();

    let stable = RuntimeExportId::new(ModuleId::new(7), RuntimeExportKind::Function, "entry");

    assert_eq!(
        stable,
        RuntimeExportId::new(ModuleId::new(7), RuntimeExportKind::Function, "entry")
    );
    assert_eq!(stable, RuntimeExportId(7_153_454_613_921_978_036));
    assert_ne!(
        stable,
        RuntimeExportId::new(ModuleId::new(8), RuntimeExportKind::Function, "entry")
    );
    assert_ne!(
        stable,
        RuntimeExportId::new(ModuleId::new(7), RuntimeExportKind::Global, "entry")
    );
    assert_ne!(
        stable,
        RuntimeExportId::new(ModuleId::new(7), RuntimeExportKind::Function, "other")
    );
}

#[test]
fn runtime_export_registry_reports_an_injected_collision_deterministically() {
    let first = RuntimeExportIdentity::new(ModuleId::new(7), RuntimeExportKind::Function, "entry");
    let second = RuntimeExportIdentity::new(ModuleId::new(8), RuntimeExportKind::Global, "entry");
    let forced_id = RuntimeExportId(42);
    let mut registry = RuntimeExportIdRegistry::default();

    assert_eq!(
        registry.register_id(forced_id, first.clone()),
        Ok(forced_id)
    );
    assert!(matches!(
        registry.register_id(forced_id, second.clone()),
        Err(RuntimeExportIdCollision {
            id,
            first: found_first,
            second: found_second,
        }) if id == forced_id && found_first == first && found_second == second
    ));
}
