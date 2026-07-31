use std::fmt;

use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModulePath(Arc<str>);

impl ModulePath {
    pub fn new(path: &str) -> Option<Self> {
        if path.contains('\0') {
            return None;
        }

        let mut segments = Vec::new();
        for segment in path.split(['/', '\\']) {
            if segment.is_empty() || segment == "." {
                continue;
            }
            if segment == ".." {
                if segments.is_empty() {
                    return None;
                }
                segments.pop();
            } else {
                segments.push(segment);
            }
        }

        let normalized = segments.join("/");

        if !normalized.ends_with(".gfs") {
            return None;
        }

        Some(Self(Arc::from(normalized)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModulePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
