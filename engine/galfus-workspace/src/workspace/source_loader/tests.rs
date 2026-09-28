use super::*;

use std::collections::HashMap;

struct MemorySourceLoader {
    sources: HashMap<ModulePath, LoadedWorkspaceSource>,
}

impl MemorySourceLoader {
    fn new(entries: impl IntoIterator<Item = (ModulePath, LoadedWorkspaceSource)>) -> Self {
        Self {
            sources: entries.into_iter().collect(),
        }
    }
}

impl WorkspaceSourceLoader for MemorySourceLoader {
    fn load_source(
        &self,
        module_path: &ModulePath,
    ) -> Result<LoadedWorkspaceSource, SourceLoadError> {
        self.sources
            .get(module_path)
            .cloned()
            .ok_or_else(|| SourceLoadError::not_found(module_path.clone()))
    }
}

#[test]
fn memory_loader_returns_source_and_preserves_its_origin() {
    let module_path = ModulePath::new("src/main.gfs").expect("valid module path");
    let loader = MemorySourceLoader::new([(
        module_path.clone(),
        LoadedWorkspaceSource::new(
            Arc::from(&b"export fn main(): i32 { return 0 }"[..]),
            ModuleOrigin::User,
        ),
    )]);

    let source = loader.load_source(&module_path).expect("source loads");

    assert_eq!(source.origin(), ModuleOrigin::User);
    assert_eq!(
        source.bytes().as_ref(),
        b"export fn main(): i32 { return 0 }"
    );
}

#[test]
fn source_load_errors_keep_the_requested_path() {
    let module_path = ModulePath::new("src/missing.gfs").expect("valid module path");
    let loader = MemorySourceLoader::new([]);

    let error = loader
        .load_source(&module_path)
        .expect_err("missing source reports an error");

    assert_eq!(error.requested_path(), &module_path);
    assert_eq!(error.module_id(), None);
    assert_eq!(error.kind(), &SourceLoadErrorKind::NotFound);
}
