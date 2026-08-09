use super::{
    BoundaryAbiVersion, CURRENT_BOUNDARY_ABI_VERSION, CURRENT_NUMERIC_SEMANTICS_VERSION,
    CURRENT_PRODUCER_VERSION, NumericSemanticsVersion, PackageCompatibilityError,
    PackageCompatibilityWarning, ProducerVersion, check_producer_compatibility,
    validate_boundary_abi, validate_numeric_semantics,
};

#[test]
fn producer_patch_difference_is_accepted() {
    let package = ProducerVersion::new(
        CURRENT_PRODUCER_VERSION.major(),
        CURRENT_PRODUCER_VERSION.minor(),
        CURRENT_PRODUCER_VERSION.patch() + 1,
    );

    assert_eq!(check_producer_compatibility(package), Ok(None));
}

#[test]
fn producer_minor_difference_returns_a_warning() {
    let package = ProducerVersion::new(
        CURRENT_PRODUCER_VERSION.major(),
        CURRENT_PRODUCER_VERSION.minor() + 1,
        0,
    );

    assert_eq!(
        check_producer_compatibility(package),
        Ok(Some(PackageCompatibilityWarning::MinorVersionMismatch {
            supported: CURRENT_PRODUCER_VERSION,
            actual: package,
        }))
    );
}

#[test]
fn producer_major_difference_is_rejected() {
    let package = ProducerVersion::new(CURRENT_PRODUCER_VERSION.major() + 1, 0, 0);

    assert_eq!(
        check_producer_compatibility(package),
        Err(PackageCompatibilityError {
            supported: CURRENT_PRODUCER_VERSION,
            actual: package,
        })
    );
}

#[test]
fn boundary_abi_uses_the_common_compatibility_policy() {
    let newer_minor = BoundaryAbiVersion::new(
        CURRENT_BOUNDARY_ABI_VERSION.major(),
        CURRENT_BOUNDARY_ABI_VERSION.minor() + 1,
        0,
    );

    assert!(matches!(
        validate_boundary_abi(newer_minor),
        Ok(Some(
            PackageCompatibilityWarning::MinorVersionMismatch { .. }
        ))
    ));

    let newer_minor = NumericSemanticsVersion::new(
        CURRENT_NUMERIC_SEMANTICS_VERSION.major(),
        CURRENT_NUMERIC_SEMANTICS_VERSION.minor() + 1,
        0,
    );
    assert!(matches!(
        validate_numeric_semantics(newer_minor),
        Ok(Some(
            PackageCompatibilityWarning::MinorVersionMismatch { .. }
        ))
    ));
}

#[test]
fn numeric_semantics_rejects_a_different_major_version() {
    let incompatible =
        NumericSemanticsVersion::new(CURRENT_NUMERIC_SEMANTICS_VERSION.major() + 1, 0, 0);

    assert!(validate_numeric_semantics(incompatible).is_err());
}
