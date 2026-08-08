use super::{
    Version, VersionCompatibilityError, VersionCompatibilityWarning, VersionParseError,
    check_version_compatibility,
};

#[test]
fn parses_semantic_versions() {
    assert_eq!("1.2.3".parse(), Ok(Version::new(1, 2, 3)));
    assert_eq!(
        "1.2".parse::<Version>(),
        Err(VersionParseError("1.2".to_string()))
    );
}

#[test]
fn compatibility_ignores_patch_changes() {
    assert_eq!(
        check_version_compatibility(Version::new(1, 2, 3), Version::new(1, 2, 4)),
        Ok(None)
    );
}

#[test]
fn compatibility_warns_for_minor_changes() {
    let supported = Version::new(1, 2, 3);
    let actual = Version::new(1, 3, 0);
    assert_eq!(
        check_version_compatibility(supported, actual),
        Ok(Some(VersionCompatibilityWarning::MinorVersionMismatch {
            supported,
            actual,
        }))
    );
}

#[test]
fn compatibility_rejects_major_changes() {
    let supported = Version::new(1, 2, 3);
    let actual = Version::new(2, 0, 0);
    assert_eq!(
        check_version_compatibility(supported, actual),
        Err(VersionCompatibilityError { supported, actual })
    );
}
