#[cfg(test)]
mod tests;

use galfus_core::{
    Version, VersionCompatibilityError, VersionCompatibilityWarning, check_version_compatibility,
};

pub type ProducerVersion = Version;
pub type BoundaryAbiVersion = Version;
pub type NumericSemanticsVersion = Version;
pub type PackageCompatibilityWarning = VersionCompatibilityWarning;
pub type PackageCompatibilityError = VersionCompatibilityError;

macro_rules! version_from_env {
    ($name:literal) => {
        Version::new(
            parse_u16(env!(concat!($name, "_MAJOR"))),
            parse_u16(env!(concat!($name, "_MINOR"))),
            parse_u16(env!(concat!($name, "_PATCH"))),
        )
    };
}

/// The version of the Galfus crate that produced a package image.
pub const CURRENT_PRODUCER_VERSION: ProducerVersion = version_from_env!("GALFUS_PRODUCER_VERSION");

/// The boundary ABI accepted by this release, configured in the workspace manifest.
pub const CURRENT_BOUNDARY_ABI_VERSION: BoundaryAbiVersion =
    version_from_env!("GALFUS_BOUNDARY_ABI_VERSION");

/// The numeric semantics version accepted by this release, configured in the workspace manifest.
pub const CURRENT_NUMERIC_SEMANTICS_VERSION: NumericSemanticsVersion =
    version_from_env!("GALFUS_NUMERIC_SEMANTICS_VERSION");

/// Applies the producer-version compatibility policy for a package image.
pub fn check_producer_compatibility(
    package: ProducerVersion,
) -> Result<Option<PackageCompatibilityWarning>, PackageCompatibilityError> {
    check_version_compatibility(CURRENT_PRODUCER_VERSION, package)
}

/// Validates that a package uses a compatible boundary ABI.
pub fn validate_boundary_abi(
    actual: BoundaryAbiVersion,
) -> Result<Option<PackageCompatibilityWarning>, PackageCompatibilityError> {
    check_version_compatibility(CURRENT_BOUNDARY_ABI_VERSION, actual)
}

/// Validates that a package uses compatible numeric semantics.
pub fn validate_numeric_semantics(
    actual: NumericSemanticsVersion,
) -> Result<Option<PackageCompatibilityWarning>, PackageCompatibilityError> {
    check_version_compatibility(CURRENT_NUMERIC_SEMANTICS_VERSION, actual)
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
