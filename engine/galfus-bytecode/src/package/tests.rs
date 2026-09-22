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
    CURRENT_PACKAGE_FORMAT_VERSION, ConstantPool, ImportEdge, ImportResolutionMode, ModuleCatalog,
    PackageFormatVersion, derive_module_catalog,
};

const PACKAGE_FORMAT_V2_FIXTURE_HEX: &str = "0201000000000000000000000000000000000000000004746573740007666978747572650000000080ade204808080800180088008808001802080088080800480800180208010800880088040000000000000000000000000000000000000000000000200000000000000000000000000000000000000020100000000000000000000000000000000000001000000000000000000000000000000000000000100000000000000000000000000000000000000";

fn package_format_v2_fixture() -> Vec<u8> {
    PACKAGE_FORMAT_V2_FIXTURE_HEX
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_digit(pair[0]).expect("fixture contains hexadecimal digits");
            let low = hex_digit(pair[1]).expect("fixture contains hexadecimal digits");
            (high << 4) | low
        })
        .collect()
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

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

#[test]
fn package_image_owns_its_graph_manifest_and_versions() {
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
fn package_image_rejects_unreachable_and_duplicate_adapter_requirements() {
    let graph = graph(&["src/main.gfs", "graphics.gfp"], Vec::new());
    let entry = PackageEntryPoint::new(
        ModulePath::new("src/main.gfs").expect("valid module path"),
        "main",
    );

    assert!(matches!(
        package_image(
            graph.clone(),
            target(),
            Some(entry.clone()),
            crate::PackageMetadata { name: "test".into(), version: None, author: None, email: None, description: None },
            galfus_contract::LimitsMetadata::default(),
            vec![requirement("graphics.gfp")],
            Vec::new(),
        ),
        Err(PackageValidationError::UnexpectedAdapterRequirement { proxy_module })
            if proxy_module == "graphics.gfp"
    ));
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
fn package_bytecode_round_trip_rebuilds_graph_indexes() {
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

    assert_eq!(decoded.graph().len(), 2);
    assert_eq!(decoded.catalog().len(), 2);
    assert!(decoded.catalog().get(ModuleId::new(1)).is_some());
    assert_eq!(
        decoded
            .graph()
            .deps_of(ModuleId::new(1))
            .collect::<Vec<_>>(),
        vec![ModuleId::new(2)]
    );
    assert_eq!(decoded.to_bytecode().expect("package re-encodes"), bytes);
}

#[test]
fn package_format_v2_fixture_decodes_and_upgrades_to_the_current_format() {
    let fixture = package_format_v2_fixture();
    let decoded = PackageImage::from_bytecode(fixture.as_slice()).expect("fixture package decodes");
    let encoded = decoded.to_bytecode().expect("upgraded package encodes");

    assert_eq!(
        decoded.versions().package_format(),
        CURRENT_PACKAGE_FORMAT_VERSION
    );
    assert_eq!(
        decoded.import_resolution_mode(),
        ImportResolutionMode::Legacy
    );
    assert!(decoded.catalog().is_empty());
    assert_ne!(encoded, fixture);
    let redecoded =
        PackageImage::from_bytecode(encoded.as_slice()).expect("upgraded package decodes");
    assert_eq!(
        redecoded.import_resolution_mode(),
        ImportResolutionMode::Direct
    );
    assert_eq!(
        redecoded
            .to_bytecode()
            .expect("upgraded package re-encodes"),
        encoded
    );
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
