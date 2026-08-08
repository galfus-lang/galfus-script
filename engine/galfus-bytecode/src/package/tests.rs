use galfus_contract::{CURRENT_BOUNDARY_ABI_VERSION, CURRENT_PRODUCER_VERSION};
use galfus_core::ModulePath;

use super::{PackageEntryPoint, PackageImage};
use crate::{CURRENT_BYTECODE_FORMAT_VERSION, CURRENT_PACKAGE_FORMAT_VERSION};

#[test]
fn package_image_owns_its_graph_manifest_and_versions() {
    let entry = PackageEntryPoint::new(
        ModulePath::new("src/main.gfs").expect("valid module path"),
        "main",
    );
    let package = PackageImage::new(crate::BytecodeGraph::new(), Some(entry), Vec::new());

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
