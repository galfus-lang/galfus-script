use galfus_core::VersionCompatibilityWarning;

use super::{
    BytecodeFormatVersion, CURRENT_BYTECODE_FORMAT_VERSION, CURRENT_PACKAGE_FORMAT_VERSION,
    PackageFormatVersion, validate_bytecode_format, validate_package_format,
};

#[test]
fn formats_are_read_from_workspace_metadata() {
    assert_eq!(
        CURRENT_BYTECODE_FORMAT_VERSION,
        BytecodeFormatVersion::new(2, 0, 0)
    );
    assert_eq!(
        CURRENT_PACKAGE_FORMAT_VERSION,
        PackageFormatVersion::new(1, 0, 0)
    );
}

#[test]
fn package_format_warns_for_newer_minor_versions() {
    let newer_minor = PackageFormatVersion::new(
        CURRENT_PACKAGE_FORMAT_VERSION.major(),
        CURRENT_PACKAGE_FORMAT_VERSION.minor() + 1,
        0,
    );
    assert!(matches!(
        validate_package_format(newer_minor),
        Ok(Some(
            VersionCompatibilityWarning::MinorVersionMismatch { .. }
        ))
    ));
}

#[test]
fn bytecode_format_rejects_different_major_versions() {
    let newer_major = BytecodeFormatVersion::new(CURRENT_BYTECODE_FORMAT_VERSION.major() + 1, 0, 0);
    assert!(validate_bytecode_format(newer_major).is_err());
}
