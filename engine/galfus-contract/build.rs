use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let workspace_manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../..")
        .join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", workspace_manifest.display());

    let manifest = fs::read_to_string(&workspace_manifest).unwrap();
    let manifest = manifest.parse::<toml::Table>().unwrap();
    let boundary_abi_version =
        manifest["workspace"]["metadata"]["galfus"]["package-image"]["boundary-abi-version"]
            .as_str()
            .unwrap();
    emit_version("GALFUS_BOUNDARY_ABI_VERSION", boundary_abi_version);
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");
    emit_version(
        "GALFUS_PRODUCER_VERSION",
        &env::var("CARGO_PKG_VERSION").unwrap(),
    );
}

fn emit_version(name: &str, version: &str) {
    let mut components = version.split('.');
    let major = components.next().unwrap();
    let minor = components.next().unwrap();
    let patch = components.next().unwrap();
    assert!(components.next().is_none(), "invalid version `{version}`");

    println!("cargo:rustc-env={name}_MAJOR={major}");
    println!("cargo:rustc-env={name}_MINOR={minor}");
    println!("cargo:rustc-env={name}_PATCH={patch}");
}
