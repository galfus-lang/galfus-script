#[cfg(test)]
mod tests;

use galfus_core::{
    Version, VersionCompatibilityError, VersionCompatibilityWarning, check_version_compatibility,
};

pub type BytecodeFormatVersion = Version;
pub type PackageFormatVersion = Version;
pub type BytecodeFormatError = VersionCompatibilityError;
pub type PackageFormatError = VersionCompatibilityError;

macro_rules! version_from_env {
    ($name:literal) => {
        Version::new(
            parse_u16(env!(concat!($name, "_MAJOR"))),
            parse_u16(env!(concat!($name, "_MINOR"))),
            parse_u16(env!(concat!($name, "_PATCH"))),
        )
    };
}

/// The only bytecode format this runtime release can interpret.
pub const CURRENT_BYTECODE_FORMAT_VERSION: BytecodeFormatVersion =
    version_from_env!("GALFUS_BYTECODE_FORMAT_VERSION");

/// The package format accepted by this release, configured in the workspace manifest.
pub const CURRENT_PACKAGE_FORMAT_VERSION: PackageFormatVersion =
    version_from_env!("GALFUS_PACKAGE_FORMAT_VERSION");

/// Validates that a graph uses a compatible bytecode format.
pub fn validate_bytecode_format(
    actual: BytecodeFormatVersion,
) -> Result<Option<VersionCompatibilityWarning>, BytecodeFormatError> {
    check_version_compatibility(CURRENT_BYTECODE_FORMAT_VERSION, actual)
}

/// Validates that a package uses a compatible outer format.
pub fn validate_package_format(
    actual: PackageFormatVersion,
) -> Result<Option<VersionCompatibilityWarning>, PackageFormatError> {
    check_version_compatibility(CURRENT_PACKAGE_FORMAT_VERSION, actual)
}

const fn parse_u16(value: &str) -> u16 {
    let bytes = value.as_bytes();
    let mut index = 0;
    let mut result = 0u16;

    while index < bytes.len() {
        let byte = bytes[index];
        assert!(byte >= b'0' && byte <= b'9');
        result = match result.checked_mul(10) {
            Some(result) => result,
            None => panic!("version exceeds u16"),
        };
        result = match result.checked_add((byte - b'0') as u16) {
            Some(result) => result,
            None => panic!("version exceeds u16"),
        };
        index += 1;
    }

    result
}
