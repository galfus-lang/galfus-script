use super::*;

use crate::source_store::ModuleOrigin;
use crate::state::WorkspaceError;
use crate::workspace::{
    LoadedWorkspaceSource, SourceLoadError, SourceLoadErrorKind, SourceRequestResult,
    WorkspaceSourceLoader,
};
use galfus_core::ModulePath;
use std::sync::Arc;

struct SingleSourceLoader {
    module_path: ModulePath,
    source: LoadedWorkspaceSource,
}

impl WorkspaceSourceLoader for SingleSourceLoader {
    fn load_source(
        &self,
        module_path: &ModulePath,
    ) -> Result<LoadedWorkspaceSource, SourceLoadError> {
        if module_path == &self.module_path {
            Ok(self.source.clone())
        } else {
            Err(SourceLoadError::not_found(module_path.clone()))
        }
    }
}

#[test]
fn workspace_requests_memory_backed_sources_by_canonical_path() {
    let mut workspace = Workspace::new();
    let module_path = ModulePath::new("src/main.gfs").expect("valid module path");

    let error = workspace
        .request_source(&module_path)
        .expect_err("missing loader is explicit");
    let WorkspaceError::SourceLoad(error) = error else {
        panic!("source request returns a source load error");
    };
    assert_eq!(error.requested_path(), &module_path);
    assert!(error.module_id().is_some());
    assert_eq!(error.kind(), &SourceLoadErrorKind::LoaderUnavailable);

    workspace.set_source_loader(Arc::new(SingleSourceLoader {
        module_path: module_path.clone(),
        source: LoadedWorkspaceSource::new(
            Arc::from(&b"export fn main(): i32 { return 0 }"[..]),
            ModuleOrigin::Builtin,
        ),
    }));

    let first = workspace
        .request_source(&module_path)
        .expect("loader supplies the requested source");
    let SourceRequestResult::Loaded { module_id } = first else {
        panic!("first source request loads the module");
    };
    let entry = workspace
        .source_state
        .store
        .get(&module_path)
        .expect("loaded source is stored");
    assert_eq!(entry.module_id, module_id);
    assert_eq!(entry.origin, ModuleOrigin::Builtin);

    assert_eq!(
        workspace
            .request_source(&module_path)
            .expect("loaded source is reused"),
        SourceRequestResult::AlreadyLoaded { module_id }
    );
}
