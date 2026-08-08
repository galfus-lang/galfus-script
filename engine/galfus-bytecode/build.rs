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
    let image = &manifest["workspace"]["metadata"]["galfus"]["package-image"];
    let package_format = image["format-version"].as_str().unwrap();
    let bytecode_format = image["bytecode-format-version"].as_str().unwrap();
    emit_version("GALFUS_PACKAGE_FORMAT_VERSION", package_format);
    emit_version("GALFUS_BYTECODE_FORMAT_VERSION", bytecode_format);
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
