use galfus_contract::LimitsMetadata;
use galfus_contract::{
    AdapterConfig, AdapterFunctionSignature, AdapterModuleDescriptor, AdapterModuleRequirement,
    CURRENT_BOUNDARY_ABI_VERSION, CURRENT_NUMERIC_SEMANTICS_VERSION, CURRENT_PRODUCER_VERSION,
    ExecutionTarget, SurfaceSchema,
};
use galfus_core::{ModuleId, ModulePath, SemanticRevision};

use super::{
    PackageDecodingError, PackageEntryPoint, PackageImage, PackageMetadata, PackageValidationError,
};
use crate::{
    BytecodeGraph, BytecodeModule, BytecodeNode, CURRENT_BYTECODE_FORMAT_VERSION,
    CURRENT_PACKAGE_FORMAT_VERSION, ConstantPool, ImportEdge, ModuleCatalog, ModuleChunk,
    ModuleChunkStore, ModuleChunkStoreError, ModuleDescriptor, PackageFormatVersion,
    derive_module_catalog,
};

fn graph(paths: &[&str], edges: Vec<ImportEdge>) -> BytecodeGraph {
    BytecodeGraph::from_modules(
        SemanticRevision::new(1),
        paths
            .iter()
            .enumerate()
            .map(|(index, path)| BytecodeNode {
                id: ModuleId::new(index as u32 + 1),
                path: ModulePath::new(path).expect("valid module path"),
                semantic_revision: SemanticRevision::new(1),
                module: BytecodeModule {
                    name: (*path).to_string(),
                    global_count: 0,
                    constants: ConstantPool::default(),
                    functions: Vec::new(),
                    types: Vec::new(),
                    struct_layouts: Vec::new(),
                    choice_layouts: Vec::new(),
                    imports: Vec::new(),
                    exports: Vec::new(),
                    init_func_idx: None,
                },
                metadata: None,
            })
            .collect(),
        edges,
    )
    .expect("valid graph")
}

fn requirement(proxy_module: &str) -> AdapterModuleRequirement {
    AdapterModuleRequirement {
        proxy_module: proxy_module.to_string(),
        descriptor: AdapterModuleDescriptor {
            adapter: "test".to_string(),
            config: AdapterConfig::new(),
            targets: Vec::new(),
            exports: Vec::new(),
        },
        boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
    }
}

fn target() -> ExecutionTarget {
    ExecutionTarget::new("test").expect("valid target")
}

fn package_image(
    graph: BytecodeGraph,
    target: ExecutionTarget,
    entry_point: Option<PackageEntryPoint>,
    metadata: PackageMetadata,
    limits: LimitsMetadata,
    adapter_requirements: Vec<AdapterModuleRequirement>,
    provider_requirements: Vec<galfus_contract::ProviderModuleRequirement>,
) -> Result<PackageImage, PackageValidationError> {
    let catalog = derive_module_catalog(&graph).map_err(|error| {
        PackageValidationError::CatalogDerivation {
            reason: error.to_string(),
        }
    })?;
    PackageImage::try_new(
        graph,
        catalog,
        target,
        entry_point,
        metadata,
        limits,
        adapter_requirements,
        provider_requirements,
    )
}

fn fixture_package() -> PackageImage {
    package_image(
        BytecodeGraph::new(),
        target(),
        None,
        PackageMetadata {
            name: "fixture".into(),
            version: None,
            author: None,
            email: None,
            description: None,
        },
        LimitsMetadata::default(),
        Vec::new(),
        Vec::new(),
    )
    .expect("fixture package is valid")
}

fn chunk(graph: &BytecodeGraph, catalog: &ModuleCatalog, module_id: ModuleId) -> ModuleChunk {
    ModuleChunk::from_node(
        graph.format_version(),
        graph.get(module_id).expect("graph module exists").clone(),
        catalog.get(module_id).expect("catalog descriptor exists"),
    )
    .expect("chunk is valid")
}

#[test]
fn package_image_owns_its_transient_graph_manifest_and_versions() {
    let entry = PackageEntryPoint::new(
        ModulePath::new("src/main.gfs").expect("valid module path"),
        "main",
    );
    let package = package_image(
        crate::BytecodeGraph::new(),
        target(),
        Some(entry),
        crate::PackageMetadata {
            name: "test".into(),
            version: None,
            author: None,
            email: None,
            description: None,
        },
        galfus_contract::LimitsMetadata::default(),
        Vec::new(),
        Vec::new(),
    )
    .expect("empty graph has no adapter requirements");

    assert!(package.graph().is_empty());
    assert_eq!(package.adapter_requirements(), []);
    assert_eq!(
        package.entry_point().map(PackageEntryPoint::function_name),
        Some("main")
    );
    assert_eq!(package.versions().producer(), CURRENT_PRODUCER_VERSION);
    assert_eq!(
        package.versions().package_format(),
        CURRENT_PACKAGE_FORMAT_VERSION
    );
    assert_eq!(
        package.versions().bytecode_format(),
        CURRENT_BYTECODE_FORMAT_VERSION
    );
    assert_eq!(
        package.versions().boundary_abi(),
        CURRENT_BOUNDARY_ABI_VERSION
    );
    assert_eq!(
        package.versions().numeric_semantics(),
        CURRENT_NUMERIC_SEMANTICS_VERSION
    );
}

#[test]
fn package_chunks_are_canonical_and_exactly_cover_the_catalog() {
    let graph = graph(&["src/first.gfs", "src/second.gfs"], Vec::new());
    let catalog = derive_module_catalog(&graph).expect("catalog derives");
    let first = chunk(&graph, &catalog, ModuleId::new(1));
    let second = chunk(&graph, &catalog, ModuleId::new(2));
    let sorted = ModuleChunkStore::new(vec![first.clone(), second.clone()])
        .expect("unique chunks form a store");
    let reversed = ModuleChunkStore::new(vec![second, first]).expect("unique chunks form a store");

    let metadata = PackageMetadata {
        name: "test".into(),
        version: None,
        author: None,
        email: None,
        description: None,
    };
    let first_package = PackageImage::try_new_with_chunks(
        graph.clone(),
        catalog.clone(),
        sorted,
        target(),
        None,
        metadata.clone(),
        LimitsMetadata::default(),
        Vec::new(),
        Vec::new(),
    )
    .expect("complete package is valid");
    let second_package = PackageImage::try_new_with_chunks(
        graph,
        catalog.clone(),
        reversed,
        target(),
        None,
        metadata,
        LimitsMetadata::default(),
        Vec::new(),
        Vec::new(),
    )
    .expect("complete package is valid");

    assert_eq!(first_package.chunks().len(), catalog.len());
    assert!(first_package.chunks().get(ModuleId::new(1)).is_some());
    assert!(first_package.chunks().get(ModuleId::new(2)).is_some());
    assert_eq!(
        first_package
            .canonical_bytes()
            .expect("package encodes canonically"),
        second_package
            .canonical_bytes()
            .expect("package encodes canonically")
    );
}

#[test]
fn package_rejects_duplicate_or_missing_chunks() {
    let graph = graph(&["src/first.gfs", "src/second.gfs"], Vec::new());
    let catalog = derive_module_catalog(&graph).expect("catalog derives");
    let first = chunk(&graph, &catalog, ModuleId::new(1));

    assert!(matches!(
        ModuleChunkStore::new(vec![first.clone(), first.clone()]),
        Err(ModuleChunkStoreError::DuplicateModuleId { module_id }) if module_id == ModuleId::new(1)
    ));

    let missing_second = ModuleChunkStore::new(vec![first]).expect("single chunk is unique");
    assert!(matches!(
        PackageImage::try_new_with_chunks(
            graph,
            catalog,
            missing_second,
            target(),
            None,
            PackageMetadata {
                name: "test".into(),
                version: None,
                author: None,
                email: None,
                description: None,
            },
            LimitsMetadata::default(),
            Vec::new(),
            Vec::new(),
        ),
        Err(PackageValidationError::ChunkCatalogMismatch { .. })
    ));
}

#[test]
fn package_image_rejects_a_missing_reachable_adapter_requirement() {
    let graph = graph(
        &["src/main.gfs", "graphics.gfp"],
        vec![ImportEdge {
            from: ModuleId::new(1),
            to: ModuleId::new(2),
        }],
    );
    let entry = PackageEntryPoint::new(
        ModulePath::new("src/main.gfs").expect("valid module path"),
        "main",
    );

    assert!(matches!(
        package_image(graph, target(), Some(entry), PackageMetadata { name: "test".into(), version: None, author: None, email: None, description: None }, LimitsMetadata::default(), Vec::new(), Vec::new()),
        Err(PackageValidationError::MissingAdapterRequirement { proxy_module })
            if proxy_module == "graphics.gfp"
    ));
}

#[test]
fn package_image_retains_unreachable_and_rejects_duplicate_adapter_requirements() {
    let graph = graph(&["src/main.gfs", "graphics.gfp"], Vec::new());
    let entry = PackageEntryPoint::new(
        ModulePath::new("src/main.gfs").expect("valid module path"),
        "main",
    );

    assert!(
        package_image(
            graph.clone(),
            target(),
            Some(entry.clone()),
            crate::PackageMetadata {
                name: "test".into(),
                version: None,
                author: None,
                email: None,
                description: None
            },
            galfus_contract::LimitsMetadata::default(),
            vec![requirement("graphics.gfp")],
            Vec::new(),
        )
        .is_ok()
    );
    assert!(matches!(
        package_image(
            graph,
            target(),
            Some(entry),
            PackageMetadata { name: "test".into(), version: None, author: None, email: None, description: None },
            LimitsMetadata::default(),
            vec![requirement("graphics.gfp"), requirement("graphics.gfp")],
            Vec::new(),
        ),
        Err(PackageValidationError::DuplicateAdapterRequirement { proxy_module })
            if proxy_module == "graphics.gfp"
    ));
}

#[test]
fn package_rejects_a_descriptor_capability_absent_from_the_manifest() {
    let graph = graph(
        &["src/main.gfs", "src/library.gfs"],
        vec![ImportEdge {
            from: ModuleId::new(1),
            to: ModuleId::new(2),
        }],
    );
    let base_catalog = derive_module_catalog(&graph).expect("base catalog derives");
    let main = base_catalog.get(ModuleId::new(1)).expect("main descriptor");
    let provisional = ModuleDescriptor::new_with_capability_requirements(
        main.module_id(),
        main.module_path().clone(),
        main.dependencies().to_vec(),
        vec![ModuleId::new(2)],
        Vec::new(),
        main.exports().to_vec(),
        main.has_initializer(),
        galfus_contract::ContentHash::of(&[]),
        main.chunk_hash(),
    )
    .expect("provisional descriptor is valid");
    let main = ModuleDescriptor::new_with_capability_requirements(
        provisional.module_id(),
        provisional.module_path().clone(),
        provisional.dependencies().to_vec(),
        provisional.provider_modules().to_vec(),
        provisional.adapter_proxy_modules().to_vec(),
        provisional.exports().to_vec(),
        provisional.has_initializer(),
        provisional
            .computed_interface_hash()
            .expect("interface hash serializes"),
        provisional.chunk_hash(),
    )
    .expect("descriptor is valid");
    let catalog = ModuleCatalog::new(vec![
        main,
        base_catalog
            .get(ModuleId::new(2))
            .expect("library descriptor")
            .clone(),
    ])
    .expect("catalog is valid");

    assert!(matches!(
        PackageImage::try_new(
            graph,
            catalog,
            target(),
            None,
            PackageMetadata {
                name: "test".into(),
                version: None,
                author: None,
                email: None,
                description: None,
            },
            LimitsMetadata::default(),
            Vec::new(),
            Vec::new(),
        ),
        Err(PackageValidationError::UndeclaredProviderModuleRequirement {
            module_id,
            provider_module,
        }) if module_id == ModuleId::new(1) && provider_module == ModuleId::new(2)
    ));
}

#[test]
fn package_image_canonicalizes_adapter_requirement_and_export_order() {
    let mut beta = requirement("beta.gfp");
    beta.descriptor.exports = vec![
        AdapterFunctionSignature {
            name: "zeta".to_string(),
            is_async: true,
            parameter_types: vec![SurfaceSchema::I32],
            return_type: SurfaceSchema::I32,
        },
        AdapterFunctionSignature {
            name: "alpha".to_string(),
            is_async: true,
            parameter_types: Vec::new(),
            return_type: SurfaceSchema::Null,
        },
    ];
    let package = package_image(
        graph(&["alpha.gfp", "beta.gfp"], Vec::new()),
        target(),
        None,
        crate::PackageMetadata {
            name: "test".into(),
            version: None,
            author: None,
            email: None,
            description: None,
        },
        galfus_contract::LimitsMetadata::default(),
        vec![beta.clone(), requirement("alpha.gfp")],
        Vec::new(),
    )
    .expect("complete adapter manifest");

    assert_eq!(
        package
            .adapter_requirements()
            .iter()
            .map(|requirement| requirement.proxy_module.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha.gfp", "beta.gfp"]
    );
    assert_eq!(
        package.adapter_requirements()[1]
            .descriptor
            .exports
            .iter()
            .map(|export| export.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "zeta"]
    );

    let same_package = package_image(
        graph(&["alpha.gfp", "beta.gfp"], Vec::new()),
        target(),
        None,
        crate::PackageMetadata {
            name: "test".into(),
            version: None,
            author: None,
            email: None,
            description: None,
        },
        galfus_contract::LimitsMetadata::default(),
        vec![requirement("alpha.gfp"), beta],
        Vec::new(),
    )
    .expect("complete adapter manifest");
    assert_eq!(
        package.content_hash().expect("canonical package hash"),
        same_package.content_hash().expect("canonical package hash")
    );
}

#[test]
fn package_content_hash_changes_for_execution_relevant_data() {
    let first = package_image(
        graph(&["main.gfs"], Vec::new()),
        target(),
        None,
        crate::PackageMetadata {
            name: "test".into(),
            version: None,
            author: None,
            email: None,
            description: None,
        },
        galfus_contract::LimitsMetadata::default(),
        Vec::new(),
        Vec::new(),
    )
    .expect("valid package");
    let second = package_image(
        graph(&["main.gfs"], Vec::new()),
        ExecutionTarget::new("other").expect("valid target"),
        None,
        crate::PackageMetadata {
            name: "test".into(),
            version: None,
            author: None,
            email: None,
            description: None,
        },
        galfus_contract::LimitsMetadata::default(),
        Vec::new(),
        Vec::new(),
    )
    .expect("valid package");

    assert_ne!(
        first.content_hash().expect("canonical package hash"),
        second.content_hash().expect("canonical package hash")
    );
}

#[test]
fn package_bytecode_round_trip_uses_catalog_and_chunks_without_a_graph() {
    let package = package_image(
        graph(
            &["src/main.gfs", "src/dependency.gfs"],
            vec![ImportEdge {
                from: ModuleId::new(1),
                to: ModuleId::new(2),
            }],
        ),
        target(),
        None,
        crate::PackageMetadata {
            name: "test".into(),
            version: None,
            author: None,
            email: None,
            description: None,
        },
        galfus_contract::LimitsMetadata::default(),
        Vec::new(),
        Vec::new(),
    )
    .expect("valid package");

    let bytes = package.to_bytecode().expect("package encodes");
    let decoded = PackageImage::from_bytecode(bytes.as_slice()).expect("package decodes");

    assert!(decoded.graph.is_none());
    assert_eq!(decoded.catalog().len(), 2);
    assert_eq!(decoded.chunks().len(), 2);
    assert!(decoded.catalog().get(ModuleId::new(1)).is_some());
    assert_eq!(
        decoded
            .catalog()
            .get(ModuleId::new(1))
            .expect("entry descriptor exists")
            .dependencies()
            .iter()
            .map(|dependency| dependency.module_id())
            .collect::<Vec<_>>(),
        vec![ModuleId::new(2)]
    );
    assert_eq!(decoded.to_bytecode().expect("package re-encodes"), bytes);
}

#[test]
fn package_decoder_rejects_the_previous_format_without_migration() {
    let mut package = fixture_package();
    let previous_format =
        PackageFormatVersion::new(CURRENT_PACKAGE_FORMAT_VERSION.major() - 1, 0, 0);
    package.versions.package_format = previous_format;
    let bytes = postcard::to_stdvec(&package).expect("previous package format encodes");

    assert!(matches!(
        PackageImage::from_bytecode(bytes.as_slice()),
        Err(PackageDecodingError::UnsupportedPackageFormat { supported, actual })
            if supported == CURRENT_PACKAGE_FORMAT_VERSION && actual == previous_format
    ));
}

#[test]
fn package_image_rejects_a_catalog_that_does_not_describe_its_graph() {
    let graph = graph(&["main.gfs"], Vec::new());

    assert!(matches!(
        PackageImage::try_new(
            graph,
            ModuleCatalog::new(Vec::new()).expect("empty catalog is valid"),
            target(),
            None,
            PackageMetadata {
                name: "test".into(),
                version: None,
                author: None,
                email: None,
                description: None,
            },
            LimitsMetadata::default(),
            Vec::new(),
            Vec::new(),
        ),
        Err(PackageValidationError::CatalogGraphMismatch)
    ));
}

#[test]
fn package_decoder_rejects_future_format_before_bytecode_validation() {
    let mut package = fixture_package();
    let future_format = PackageFormatVersion::new(
        CURRENT_PACKAGE_FORMAT_VERSION.major(),
        CURRENT_PACKAGE_FORMAT_VERSION.minor() + 1,
        0,
    );
    package.versions.package_format = future_format;
    package.versions.bytecode_format = PackageFormatVersion::new(0, 0, 0);
    let bytes = package.to_bytecode().expect("future package encodes");

    assert!(matches!(
        PackageImage::from_bytecode(bytes.as_slice()),
        Err(PackageDecodingError::UnsupportedPackageFormat { supported, actual })
            if supported == CURRENT_PACKAGE_FORMAT_VERSION && actual == future_format
    ));
}

#[test]
fn package_bytecode_rejects_malformed_input() {
    assert!(matches!(
        PackageImage::from_bytecode(&[0xff, 0xff]),
        Err(PackageDecodingError::Postcard(_))
    ));
}
