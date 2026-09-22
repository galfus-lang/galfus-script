use super::{
    ModuleCatalog, ModuleCatalogError, ModuleDependency, ModuleDescriptor, ModuleDescriptorError,
    ModuleExportDescriptor,
};
use galfus_contract::ContentHash;
use galfus_core::{ModuleId, ModulePath, RuntimeExportId, RuntimeExportKind};

fn hash(value: &[u8]) -> ContentHash {
    ContentHash::of(value)
}

fn export(module_id: ModuleId, kind: RuntimeExportKind, name: &str) -> ModuleExportDescriptor {
    ModuleExportDescriptor::new(RuntimeExportId::new(module_id, kind, name), name, kind)
}

fn descriptor(
    dependencies: Vec<ModuleDependency>,
    exports: Vec<ModuleExportDescriptor>,
) -> Result<ModuleDescriptor, ModuleDescriptorError> {
    module_descriptor(ModuleId::new(8), "src/example.gfs", dependencies, exports)
}

fn module_descriptor(
    module_id: ModuleId,
    path: &str,
    dependencies: Vec<ModuleDependency>,
    exports: Vec<ModuleExportDescriptor>,
) -> Result<ModuleDescriptor, ModuleDescriptorError> {
    ModuleDescriptor::new(
        module_id,
        ModulePath::new(path).expect("valid module path"),
        dependencies,
        exports,
        true,
        hash(b"interface"),
        hash(b"chunk"),
    )
}

#[derive(serde::Serialize)]
struct UncheckedModuleExportDescriptor {
    runtime_export_id: RuntimeExportId,
    name: String,
    kind: RuntimeExportKind,
}

#[derive(serde::Serialize)]
struct UncheckedModuleDescriptor {
    module_id: ModuleId,
    module_path: ModulePath,
    dependencies: Vec<ModuleDependency>,
    exports: Vec<UncheckedModuleExportDescriptor>,
    has_initializer: bool,
    interface_hash: ContentHash,
    chunk_hash: ContentHash,
}

#[test]
fn descriptor_canonicalizes_dependencies_and_exports_before_encoding() {
    let first = descriptor(
        vec![
            ModuleDependency::new(ModuleId::new(9)),
            ModuleDependency::new(ModuleId::new(2)),
            ModuleDependency::new(ModuleId::new(4)),
        ],
        vec![
            export(ModuleId::new(8), RuntimeExportKind::Global, "state"),
            export(ModuleId::new(8), RuntimeExportKind::Function, "entry"),
        ],
    )
    .expect("descriptor is valid");
    let second = descriptor(
        vec![
            ModuleDependency::new(ModuleId::new(4)),
            ModuleDependency::new(ModuleId::new(9)),
            ModuleDependency::new(ModuleId::new(2)),
        ],
        vec![
            export(ModuleId::new(8), RuntimeExportKind::Function, "entry"),
            export(ModuleId::new(8), RuntimeExportKind::Global, "state"),
        ],
    )
    .expect("descriptor is valid");

    assert_eq!(first, second);
    assert_eq!(
        first
            .dependencies()
            .iter()
            .map(ModuleDependency::module_id)
            .collect::<Vec<_>>(),
        vec![ModuleId::new(2), ModuleId::new(4), ModuleId::new(9)]
    );
    assert_eq!(
        postcard::to_stdvec(&first).expect("descriptor encodes"),
        postcard::to_stdvec(&second).expect("descriptor encodes")
    );
}

#[test]
fn descriptor_round_trips_through_serde() {
    let descriptor = descriptor(
        vec![ModuleDependency::new(ModuleId::new(2))],
        vec![export(
            ModuleId::new(8),
            RuntimeExportKind::Function,
            "entry",
        )],
    )
    .expect("descriptor is valid");

    let encoded = postcard::to_stdvec(&descriptor).expect("descriptor encodes");
    let decoded: ModuleDescriptor =
        postcard::from_bytes(encoded.as_slice()).expect("descriptor decodes");

    assert_eq!(decoded, descriptor);
}

#[test]
fn descriptor_rejects_duplicate_dependency_ids() {
    assert!(matches!(
        descriptor(
            vec![
                ModuleDependency::new(ModuleId::new(2)),
                ModuleDependency::new(ModuleId::new(2)),
            ],
            Vec::new(),
        ),
        Err(ModuleDescriptorError::DuplicateDependency {
            module_id,
            dependency,
        }) if module_id == ModuleId::new(8) && dependency == ModuleId::new(2)
    ));
}

#[test]
fn descriptor_rejects_duplicate_runtime_export_ids() {
    let entry = export(ModuleId::new(8), RuntimeExportKind::Function, "entry");

    assert!(matches!(
        descriptor(Vec::new(), vec![entry.clone(), entry]),
        Err(ModuleDescriptorError::DuplicateRuntimeExportId {
            module_id,
            runtime_export_id,
        }) if module_id == ModuleId::new(8)
            && runtime_export_id
                == RuntimeExportId::new(ModuleId::new(8), RuntimeExportKind::Function, "entry")
    ));
}

#[test]
fn catalog_canonicalizes_descriptor_insertion_order_before_encoding() {
    let first = ModuleCatalog::new(vec![
        module_descriptor(ModuleId::new(9000), "src/late.gfs", Vec::new(), Vec::new())
            .expect("descriptor is valid"),
        module_descriptor(ModuleId::new(3), "src/early.gfs", Vec::new(), Vec::new())
            .expect("descriptor is valid"),
    ])
    .expect("catalog is valid");
    let second = ModuleCatalog::new(vec![
        module_descriptor(ModuleId::new(3), "src/early.gfs", Vec::new(), Vec::new())
            .expect("descriptor is valid"),
        module_descriptor(ModuleId::new(9000), "src/late.gfs", Vec::new(), Vec::new())
            .expect("descriptor is valid"),
    ])
    .expect("catalog is valid");

    assert_eq!(first, second);
    assert_eq!(
        first
            .iter()
            .map(ModuleDescriptor::module_id)
            .collect::<Vec<_>>(),
        vec![ModuleId::new(3), ModuleId::new(9000)]
    );
    assert_eq!(
        postcard::to_stdvec(&first).expect("catalog encodes"),
        postcard::to_stdvec(&second).expect("catalog encodes")
    );
}

#[test]
fn catalog_rebuilds_sparse_module_id_index_after_decoding() {
    let catalog = ModuleCatalog::new(vec![
        module_descriptor(ModuleId::new(30), "src/thirty.gfs", Vec::new(), Vec::new())
            .expect("descriptor is valid"),
        module_descriptor(ModuleId::new(3), "src/three.gfs", Vec::new(), Vec::new())
            .expect("descriptor is valid"),
        module_descriptor(ModuleId::new(9000), "src/large.gfs", Vec::new(), Vec::new())
            .expect("descriptor is valid"),
    ])
    .expect("catalog is valid");
    let encoded = postcard::to_stdvec(&catalog).expect("catalog encodes");
    let decoded: ModuleCatalog = postcard::from_bytes(encoded.as_slice()).expect("catalog decodes");

    assert_eq!(decoded.len(), 3);
    assert_eq!(
        decoded
            .get(ModuleId::new(30))
            .map(ModuleDescriptor::module_path)
            .map(ModulePath::as_str),
        Some("src/thirty.gfs")
    );
    assert!(decoded.get(ModuleId::new(4)).is_none());
}

#[test]
fn catalog_rejects_duplicate_ids_paths_and_absent_dependencies() {
    assert!(matches!(
        ModuleCatalog::new(vec![
            module_descriptor(ModuleId::new(4), "src/first.gfs", Vec::new(), Vec::new())
                .expect("descriptor is valid"),
            module_descriptor(ModuleId::new(4), "src/second.gfs", Vec::new(), Vec::new())
                .expect("descriptor is valid"),
        ]),
        Err(ModuleCatalogError::DuplicateModuleId { module_id }) if module_id == ModuleId::new(4)
    ));

    assert!(matches!(
        ModuleCatalog::new(vec![
            module_descriptor(ModuleId::new(4), "src/shared.gfs", Vec::new(), Vec::new())
                .expect("descriptor is valid"),
            module_descriptor(ModuleId::new(8), "src/shared.gfs", Vec::new(), Vec::new())
                .expect("descriptor is valid"),
        ]),
        Err(ModuleCatalogError::DuplicateModulePath { module_path })
            if module_path.as_str() == "src/shared.gfs"
    ));

    assert!(matches!(
        ModuleCatalog::new(vec![
            module_descriptor(
                ModuleId::new(4),
                "src/entry.gfs",
                vec![ModuleDependency::new(ModuleId::new(8))],
                Vec::new(),
            )
            .expect("descriptor is valid"),
        ]),
        Err(ModuleCatalogError::MissingDependency {
            module_id,
            dependency,
        }) if module_id == ModuleId::new(4) && dependency == ModuleId::new(8)
    ));
}

#[test]
fn catalog_rejects_duplicate_runtime_exports_from_encoded_input() {
    let module_id = ModuleId::new(8);
    let runtime_export_id = RuntimeExportId::new(module_id, RuntimeExportKind::Function, "entry");
    let encoded = postcard::to_stdvec(&vec![UncheckedModuleDescriptor {
        module_id,
        module_path: ModulePath::new("src/example.gfs").expect("valid module path"),
        dependencies: Vec::new(),
        exports: vec![
            UncheckedModuleExportDescriptor {
                runtime_export_id,
                name: "entry".to_owned(),
                kind: RuntimeExportKind::Function,
            },
            UncheckedModuleExportDescriptor {
                runtime_export_id,
                name: "entry".to_owned(),
                kind: RuntimeExportKind::Function,
            },
        ],
        has_initializer: true,
        interface_hash: hash(b"interface"),
        chunk_hash: hash(b"chunk"),
    }])
    .expect("unchecked catalog encodes");

    assert!(
        postcard::from_bytes::<ModuleCatalog>(encoded.as_slice()).is_err(),
        "catalog rejects duplicate runtime export IDs"
    );
}
