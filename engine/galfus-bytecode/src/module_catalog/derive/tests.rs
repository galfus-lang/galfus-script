use super::{
    ModuleCatalogDerivationError, derive_module_catalog,
    derive_module_catalog_with_capability_requirements,
};
use crate::{
    BytecodeGraph, BytecodeModule, BytecodeNode, ConstantPool, ExportKind, ExportSlot, ImportEdge,
    ImportKind, ImportSlot, instruction,
};
use galfus_contract::{
    AdapterConfig, AdapterModuleDescriptor, AdapterModuleRequirement, BoundaryAbiVersion,
    ProviderModuleRequirement,
};
use galfus_core::{ModuleId, ModulePath, RuntimeExportId, RuntimeExportKind, SemanticRevision};

fn module(id: ModuleId, path: &str) -> BytecodeNode {
    BytecodeNode {
        id,
        path: ModulePath::new(path).expect("valid module path"),
        semantic_revision: SemanticRevision::new(1),
        module: BytecodeModule {
            name: path.to_string(),
            global_count: 1,
            constants: ConstantPool::default(),
            functions: Vec::new(),
            types: Vec::new(),
            struct_layouts: Vec::new(),
            choice_layouts: Vec::new(),
            imports: Vec::new(),
            exports: vec![ExportSlot {
                symbol_name: "value".to_string(),
                kind: ExportKind::Global(instruction::GlobalIdx(0)),
            }],
            init_func_idx: None,
        },
        metadata: None,
    }
}

fn import_slot(target_module_id: ModuleId, module_name: &str) -> ImportSlot {
    ImportSlot {
        module_name: module_name.to_string(),
        symbol_name: "value".to_string(),
        ty: instruction::TypeIdx(0),
        kind: ImportKind::Global,
        target_module_id: Some(target_module_id),
        target_export_id: Some(RuntimeExportId::new(
            target_module_id,
            RuntimeExportKind::Global,
            "value",
        )),
    }
}

#[test]
fn derivation_matches_direct_dependencies_exports_and_initializer_flags() {
    let importer = ModuleId::new(4);
    let dependency = ModuleId::new(9);
    let mut importer_node = module(importer, "src/main.gfs");
    importer_node
        .module
        .imports
        .push(import_slot(dependency, "src/value.gfs"));
    let dependency_node = module(dependency, "src/value.gfs");
    let graph = BytecodeGraph::from_modules(
        SemanticRevision::new(1),
        vec![importer_node, dependency_node],
        vec![ImportEdge {
            from: importer,
            to: dependency,
        }],
    )
    .expect("graph is valid");

    let catalog = derive_module_catalog(&graph).expect("catalog derives");
    let importer_descriptor = catalog.get(importer).expect("importer descriptor");
    let dependency_descriptor = catalog.get(dependency).expect("dependency descriptor");

    assert_eq!(catalog.len(), graph.len());
    assert_eq!(
        importer_descriptor
            .dependencies()
            .iter()
            .map(|dependency| dependency.module_id())
            .collect::<Vec<_>>(),
        vec![dependency]
    );
    assert_eq!(dependency_descriptor.exports().len(), 1);
    assert_eq!(dependency_descriptor.exports()[0].name(), "value");
    assert_eq!(
        dependency_descriptor.exports()[0].kind(),
        RuntimeExportKind::Global
    );
    assert_eq!(
        dependency_descriptor.has_initializer(),
        graph
            .get(dependency)
            .expect("dependency graph node")
            .module()
            .init_func_idx
            .is_some()
    );
}

#[test]
fn derivation_rejects_a_direct_target_that_disagrees_with_its_import_path() {
    let importer = ModuleId::new(4);
    let direct_target = ModuleId::new(9);
    let path_target = ModuleId::new(12);
    let mut importer_node = module(importer, "src/main.gfs");
    importer_node
        .module
        .imports
        .push(import_slot(direct_target, "src/path-target.gfs"));
    let direct_target_node = module(direct_target, "src/direct-target.gfs");
    let path_target_node = module(path_target, "src/path-target.gfs");
    let graph = BytecodeGraph::from_modules(
        SemanticRevision::new(1),
        vec![importer_node, direct_target_node, path_target_node],
        vec![ImportEdge {
            from: importer,
            to: direct_target,
        }],
    )
    .expect("graph is valid until catalog derivation");

    assert!(matches!(
        derive_module_catalog(&graph),
        Err(ModuleCatalogDerivationError::DirectTargetPathMismatch {
            importer: found_importer,
            target,
            import_path,
            target_path,
        }) if found_importer == importer
            && target == direct_target
            && import_path == "src/path-target.gfs"
            && target_path.as_str() == "src/direct-target.gfs"
    ));
}

#[test]
fn capability_requirements_are_derived_from_direct_module_dependencies() {
    let importer = ModuleId::new(4);
    let provider = ModuleId::new(9);
    let adapter_proxy = ModuleId::new(12);
    let mut importer_node = module(importer, "src/main.gfs");
    importer_node
        .module
        .imports
        .push(import_slot(provider, "database.gfs"));
    importer_node
        .module
        .imports
        .push(import_slot(adapter_proxy, "graphics.gfp"));
    let graph = BytecodeGraph::from_modules(
        SemanticRevision::new(1),
        vec![
            importer_node,
            module(provider, "database.gfs"),
            module(adapter_proxy, "graphics.gfp"),
        ],
        vec![
            ImportEdge {
                from: importer,
                to: provider,
            },
            ImportEdge {
                from: importer,
                to: adapter_proxy,
            },
        ],
    )
    .expect("graph is valid");
    let adapters = vec![AdapterModuleRequirement {
        proxy_module: "graphics.gfp".to_string(),
        descriptor: AdapterModuleDescriptor {
            adapter: "graphics".to_string(),
            config: AdapterConfig::new(),
            targets: Vec::new(),
            exports: Vec::new(),
        },
        boundary_abi: BoundaryAbiVersion::new(1, 0, 0),
    }];
    let providers = vec![ProviderModuleRequirement {
        alias: "database".to_string(),
        module_path: "database".to_string(),
        schema_fingerprint: 1,
        boundary_abi: BoundaryAbiVersion::new(1, 0, 0),
        exports: Vec::new(),
    }];

    let catalog = derive_module_catalog_with_capability_requirements(&graph, &adapters, &providers)
        .expect("catalog derives");
    let descriptor = catalog.get(importer).expect("importer descriptor");

    assert_eq!(descriptor.provider_modules(), [provider]);
    assert_eq!(descriptor.adapter_proxy_modules(), [adapter_proxy]);
    assert_ne!(
        descriptor.interface_hash(),
        derive_module_catalog(&graph)
            .expect("base catalog derives")
            .get(importer)
            .expect("base importer descriptor")
            .interface_hash()
    );
    assert!(
        catalog
            .get(provider)
            .expect("provider descriptor")
            .provider_modules()
            .is_empty()
    );
}
