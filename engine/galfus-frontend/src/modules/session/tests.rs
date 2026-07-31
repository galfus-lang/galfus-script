use super::*;
use crate::modules::{SemanticImportKind, SemanticRoot, SemanticRootKind};
use galfus_core::SourceId;

fn path(value: &str) -> ModulePath {
    ModulePath::new(value).expect("valid module path")
}

#[test]
fn check_uses_the_module_ids_provided_by_the_host() {
    let utilities = SourceFile::new(
        SourceId::new(3),
        "src/utilities.gfs".to_string(),
        "export fn value(): i32 { return 1 }".to_string(),
    );
    let main = SourceFile::new(
        SourceId::new(9),
        "src/main.gfs".to_string(),
        "import { value } from './utilities'\nfn main(): i32 { return value() }".to_string(),
    );
    let sources = [
        FrontendSource {
            module_id: ModuleId::new(41),
            path: path("src/main.gfs"),
            source: &main,
        },
        FrontendSource {
            module_id: ModuleId::new(7),
            path: path("src/utilities.gfs"),
            source: &utilities,
        },
    ];
    let roots = FrontendRoots::default();
    let mut session = FrontendSession::new();

    let report = session.check(FrontendUpdate {
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &roots,
    });

    assert!(!report.diagnostics.has_errors());
    assert_eq!(session.modules[0].id(), ModuleId::new(41));
    assert_eq!(session.modules[1].id(), ModuleId::new(7));
    assert_eq!(
        session
            .semantic_graph()
            .module_by_path(&path("src/main.gfs")),
        Some(ModuleId::new(41))
    );
    assert_eq!(
        session
            .semantic_graph()
            .semantic_revision(ModuleId::new(41)),
        Some(session.modules[0].semantic_revision())
    );
    assert!(
        session.semantic_graph().import_edges().iter().any(|edge| {
            edge.from() == ModuleId::new(41) && edge.to() == Some(ModuleId::new(7))
        })
    );
}

#[test]
fn check_reprocesses_changed_modules_and_transitive_dependents_only() {
    let utilities_v1 = SourceFile::new(
        SourceId::new(3),
        "src/utilities.gfs".to_string(),
        "export fn value(): i32 { return 1 }".to_string(),
    );
    let main = SourceFile::new(
        SourceId::new(9),
        "src/main.gfs".to_string(),
        "import { value } from './utilities'\nfn main(): i32 { return value() }".to_string(),
    );
    let isolated = SourceFile::new(
        SourceId::new(12),
        "src/isolated.gfs".to_string(),
        "fn isolated(): i32 { return 0 }".to_string(),
    );
    let initial_sources = [
        FrontendSource {
            module_id: ModuleId::new(41),
            path: path("src/main.gfs"),
            source: &main,
        },
        FrontendSource {
            module_id: ModuleId::new(7),
            path: path("src/utilities.gfs"),
            source: &utilities_v1,
        },
        FrontendSource {
            module_id: ModuleId::new(13),
            path: path("src/isolated.gfs"),
            source: &isolated,
        },
    ];
    let roots = FrontendRoots::default();
    let mut session = FrontendSession::new();
    session.check(FrontendUpdate {
        source_revision: Revision::new(1),
        sources: &initial_sources,
        removed_modules: &[],
        roots: &roots,
    });
    let main_revision = session
        .semantic_graph()
        .semantic_revision(ModuleId::new(41))
        .expect("main revision");
    let isolated_revision = session
        .semantic_graph()
        .semantic_revision(ModuleId::new(13))
        .expect("isolated revision");

    let utilities_v2 = SourceFile::new(
        SourceId::new(3),
        "src/utilities.gfs".to_string(),
        "export fn value(): i32 { return 2 }".to_string(),
    );
    let update_sources = [FrontendSource {
        module_id: ModuleId::new(7),
        path: path("src/utilities.gfs"),
        source: &utilities_v2,
    }];
    let report = session.check(FrontendUpdate {
        source_revision: Revision::new(2),
        sources: &update_sources,
        removed_modules: &[],
        roots: &roots,
    });

    assert_eq!(report.changed_modules.len(), 2);
    assert!(report.changed_modules.contains(&ModuleId::new(7)));
    assert!(report.changed_modules.contains(&ModuleId::new(41)));
    assert!(!report.changed_modules.contains(&ModuleId::new(13)));
    assert!(
        session
            .semantic_graph()
            .semantic_revision(ModuleId::new(41))
            .expect("updated main revision")
            > main_revision
    );
    assert_eq!(
        session
            .semantic_graph()
            .semantic_revision(ModuleId::new(13)),
        Some(isolated_revision)
    );
}

#[test]
fn check_records_resolved_implicit_range_dependency() {
    let main = SourceFile::new(
        SourceId::new(1),
        "src/main.gfs".to_string(),
        "fn main(): i32 { for value in 0..2 { } return 0 }".to_string(),
    );
    let iterable = SourceFile::new(
        SourceId::new(2),
        "std/iterable.gfs".to_string(),
        "export fn range(start: i32, end: i32): i32 { return start }".to_string(),
    );
    let sources = [
        FrontendSource {
            module_id: ModuleId::new(1),
            path: path("src/main.gfs"),
            source: &main,
        },
        FrontendSource {
            module_id: ModuleId::new(2),
            path: path("std/iterable.gfs"),
            source: &iterable,
        },
    ];
    let roots = FrontendRoots::new(vec![SemanticRoot::new(
        SemanticRootKind::Entry,
        ModuleId::new(1),
        path("src/main.gfs"),
    )]);
    let mut session = FrontendSession::new();

    session.check(FrontendUpdate {
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &roots,
    });

    assert!(session.semantic_graph().import_edges().iter().any(|edge| {
        edge.from() == ModuleId::new(1)
            && edge.kind() == SemanticImportKind::Implicit
            && edge.target_path() == &path("std/iterable.gfs")
            && edge.to() == Some(ModuleId::new(2))
    }));
}

#[test]
fn check_records_iterable_dependency_for_array_iteration() {
    let main = SourceFile::new(
        SourceId::new(1),
        "src/main.gfs".to_string(),
        "fn main(): i32 { for value in [1, 2] { } return 0 }".to_string(),
    );
    let iterable = SourceFile::new(
        SourceId::new(2),
        "std/iterable.gfs".to_string(),
        "export fn arrayIter<T>(values: [T]): [T] { return values }".to_string(),
    );
    let sources = [
        FrontendSource {
            module_id: ModuleId::new(1),
            path: path("src/main.gfs"),
            source: &main,
        },
        FrontendSource {
            module_id: ModuleId::new(2),
            path: path("std/iterable.gfs"),
            source: &iterable,
        },
    ];
    let roots = FrontendRoots::new(vec![SemanticRoot::new(
        SemanticRootKind::Entry,
        ModuleId::new(1),
        path("src/main.gfs"),
    )]);
    let mut session = FrontendSession::new();

    session.check(FrontendUpdate {
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &roots,
    });

    assert!(session.semantic_graph().import_edges().iter().any(|edge| {
        edge.from() == ModuleId::new(1)
            && edge.kind() == SemanticImportKind::Implicit
            && edge.target_path() == &path("std/iterable.gfs")
            && edge.to() == Some(ModuleId::new(2))
    }));
}

#[test]
fn check_removes_modules_and_refreshes_dependent_edges() {
    let main = SourceFile::new(
        SourceId::new(1),
        "src/main.gfs".to_string(),
        "import { value } from './utility'\nfn main(): i32 { return value() }".to_string(),
    );
    let utility = SourceFile::new(
        SourceId::new(2),
        "src/utility.gfs".to_string(),
        "export fn value(): i32 { return 1 }".to_string(),
    );
    let initial_sources = [
        FrontendSource {
            module_id: ModuleId::new(1),
            path: path("src/main.gfs"),
            source: &main,
        },
        FrontendSource {
            module_id: ModuleId::new(2),
            path: path("src/utility.gfs"),
            source: &utility,
        },
    ];
    let roots = FrontendRoots::new(vec![SemanticRoot::new(
        SemanticRootKind::Entry,
        ModuleId::new(1),
        path("src/main.gfs"),
    )]);
    let mut session = FrontendSession::new();

    session.check(FrontendUpdate {
        source_revision: Revision::new(1),
        sources: &initial_sources,
        removed_modules: &[],
        roots: &roots,
    });
    session.check(FrontendUpdate {
        source_revision: Revision::new(2),
        sources: &[],
        removed_modules: &[ModuleId::new(2)],
        roots: &roots,
    });

    assert!(session.semantic_graph().get(ModuleId::new(2)).is_none());
    assert!(session.semantic_graph().import_edges().iter().any(|edge| {
        edge.from() == ModuleId::new(1)
            && edge.target_path() == &path("src/utility.gfs")
            && edge.to().is_none()
    }));
}
