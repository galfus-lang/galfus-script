use crate::PackageImage;
use std::sync::Arc;

/// Produces or retrieves one immutable package image.
///
/// Implementations own their source of truth. For example, a workspace compiles
/// its loaded sources while an OTA loader retrieves an already-built image.
/// Execution hosts receive the resulting image but never depend on this trait.
pub trait PackageLoader {
    type Error;

    fn load(&mut self) -> Result<Arc<PackageImage>, Self::Error>;
}
