#[cfg(test)]
mod tests;

use std::fmt;

/// A semantic version used by Galfus package and compatibility contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    major: u16,
    minor: u16,
    patch: u16,
}

impl Version {
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub const fn major(self) -> u16 {
        self.major
    }

    pub const fn minor(self) -> u16 {
        self.minor
    }

    pub const fn patch(self) -> u16 {
        self.patch
    }

    pub fn parse(value: &str) -> Result<Self, VersionParseError> {
        let mut components = value.split('.');
        let major = parse_component(components.next(), value)?;
        let minor = parse_component(components.next(), value)?;
        let patch = parse_component(components.next(), value)?;

        if components.next().is_some() {
            return Err(VersionParseError(value.to_string()));
        }

        Ok(Self::new(major, minor, patch))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl std::str::FromStr for Version {
    type Err = VersionParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionParseError(String);

impl fmt::Display for VersionParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid version `{}`; expected MAJOR.MINOR.PATCH",
            self.0
        )
    }
}

impl std::error::Error for VersionParseError {}

fn parse_component(component: Option<&str>, original: &str) -> Result<u16, VersionParseError> {
    component
        .filter(|component| !component.is_empty())
        .and_then(|component| component.parse().ok())
        .ok_or_else(|| VersionParseError(original.to_string()))
}

/// A non-fatal compatibility observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionCompatibilityWarning {
    MinorVersionMismatch { supported: Version, actual: Version },
}

/// A version mismatch that prevents a contract from being used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VersionCompatibilityError {
    pub supported: Version,
    pub actual: Version,
}

impl fmt::Display for VersionCompatibilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "major version {} is incompatible with supported version {}",
            self.actual, self.supported
        )
    }
}

impl std::error::Error for VersionCompatibilityError {}

/// Applies the Galfus compatibility policy: patch differences are accepted,
/// minor differences return a warning, and major differences are rejected.
pub fn check_version_compatibility(
    supported: Version,
    actual: Version,
) -> Result<Option<VersionCompatibilityWarning>, VersionCompatibilityError> {
    if actual.major() != supported.major() {
        return Err(VersionCompatibilityError { supported, actual });
    }

    Ok((actual.minor() != supported.minor())
        .then_some(VersionCompatibilityWarning::MinorVersionMismatch { supported, actual }))
}
