use galfus_contract::{
    AdapterConfig, AdapterFunctionSignature, AdapterModuleDescriptor, AdapterModuleRequirement,
    BoundaryType, CURRENT_BOUNDARY_ABI_VERSION, CURRENT_PRODUCER_VERSION,
};
use galfus_core::{ModuleId, ModulePath, SemanticRevision};

use super::{PackageEntryPoint, PackageImage, PackageValidationError};
use crate::{
    BytecodeGraph, BytecodeModule, BytecodeNode, CURRENT_BYTECODE_FORMAT_VERSION,
    CURRENT_PACKAGE_FORMAT_VERSION, ConstantPool, ImportEdge,
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
            exports: Vec::new(),
        },
    }
}

#[test]
fn package_image_owns_its_graph_manifest_and_versions() {
    let entry = PackageEntryPoint::new(
        ModulePath::new("src/main.gfs").expect("valid module path"),
        "main",
    );
    let package = PackageImage::try_new(crate::BytecodeGraph::new(), Some(entry), Vec::new())
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
        PackageImage::try_new(graph, Some(entry), Vec::new()),
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
        PackageImage::try_new(
            graph.clone(),
            Some(entry.clone()),
            vec![requirement("graphics.gfp")]
        ),
        Err(PackageValidationError::UnexpectedAdapterRequirement { proxy_module })
            if proxy_module == "graphics.gfp"
    ));
    assert!(matches!(
        PackageImage::try_new(
            graph,
            Some(entry),
            vec![requirement("graphics.gfp"), requirement("graphics.gfp")],
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
            parameter_types: vec![BoundaryType::I32],
            return_type: BoundaryType::I32,
        },
        AdapterFunctionSignature {
            name: "alpha".to_string(),
            is_async: true,
            parameter_types: Vec::new(),
            return_type: BoundaryType::Null,
        },
    ];
    let package = PackageImage::try_new(
        graph(&["alpha.gfp", "beta.gfp"], Vec::new()),
        None,
        vec![beta, requirement("alpha.gfp")],
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
}
