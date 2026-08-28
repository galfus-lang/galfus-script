use super::*;
use crate::modules::{SemanticImportKind, SemanticRoot, SemanticRootKind};
use galfus_contract::CapabilityCatalog;
use galfus_core::SourceId;
use std::sync::Arc;

fn io_catalog() -> Arc<CapabilityCatalog> {
    Arc::new(
        CapabilityCatalog::new(Vec::new(), Vec::new())
            .expect("the std/io provider catalog is valid"),
    )
}

fn path(value: &str) -> ModulePath {
    ModulePath::new(value).expect("valid module path")
}

#[test]
fn empty_catalog_rejects_std_io_imports() {
    let main = SourceFile::new(
        SourceId::new(1),
        "main.gfs".to_string(),
        "import { println } from \"std/io\"\nexport fn main(args: [[u8]]): i32 { return 0 }"
            .to_string(),
    );
    let sources = [FrontendSource {
        module_id: ModuleId::new(1),
        path: path("main.gfs"),
        source: &main,
        kind: FrontendModuleKind::Standard,
    }];
    let mut session = FrontendSession::new();
    let catalog = Arc::new(CapabilityCatalog::default());
    let report = session.check(FrontendUpdate {
        catalog,
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
    });

    assert!(report.diagnostics.has_errors());
}

#[test]
fn check_binds_array_destructuring_types_in_entry_function() {
    let main = SourceFile::new(
        SourceId::new(1),
        "main.gfs".to_string(),
        r#"
struct Pair { left: i32, right: i32 }

export fn main(_args: [[u8]]): i32 {
  const initial = [1, 2, 3]
  const values: [i32] = [...initial, 4, 5]
  const [first, second, ...rest] = values
  const pair = new(Pair) { left: first, right: second }
  if pair.left != 1 || pair.right != 2 { return 1 }
  if rest.length != 4 { return 2 }
  return 0
}
"#
        .to_string(),
    );
    let sources = [FrontendSource {
        module_id: ModuleId::new(1),
        path: path("main.gfs"),
        source: &main,
        kind: FrontendModuleKind::Standard,
    }];
    let mut session = FrontendSession::new();
    let report = session.check(FrontendUpdate {
        catalog: io_catalog(),
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
    });

    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
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
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(7),
            path: path("src/utilities.gfs"),
            source: &utilities,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let roots = FrontendRoots::default();
    let mut session = FrontendSession::new();

    let report = session.check(FrontendUpdate {
        catalog: io_catalog(),
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
fn check_preserves_async_future_payloads_across_imported_generic_calls() {
    let utilities = SourceFile::new(
        SourceId::new(3),
        "src/utilities.gfs".to_string(),
        "export struct Future<T> { id: i64 }\nexport fn(async) load<T>(value: T): T { return value }"
            .to_string(),
    );
    let main = SourceFile::new(
        SourceId::new(9),
        "src/main.gfs".to_string(),
        "import { Future, load } from './utilities'\nfn(async) main(): i32 { const future = load(1); return await future }".to_string(),
    );
    let sources = [
        FrontendSource {
            module_id: ModuleId::new(41),
            path: path("src/main.gfs"),
            source: &main,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(7),
            path: path("src/utilities.gfs"),
            source: &utilities,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let mut session = FrontendSession::new();

    let report = session.check(FrontendUpdate {
        catalog: io_catalog(),
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
    });

    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
}

#[test]
fn check_preserves_async_future_payloads_across_namespace_calls() {
    let utilities = SourceFile::new(
        SourceId::new(3),
        "src/utilities.gfs".to_string(),
        "export struct Future<T> { id: i64 }\nexport fn(async) load(): i32 { return 1 }"
            .to_string(),
    );
    let main = SourceFile::new(
        SourceId::new(9),
        "src/main.gfs".to_string(),
        "import utilities from './utilities'\nfn main(): i32 { return await utilities::load() }"
            .to_string(),
    );
    let sources = [
        FrontendSource {
            module_id: ModuleId::new(41),
            path: path("src/main.gfs"),
            source: &main,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(7),
            path: path("src/utilities.gfs"),
            source: &utilities,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let mut session = FrontendSession::new();

    let report = session.check(FrontendUpdate {
        catalog: io_catalog(),
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
    });

    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
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
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(7),
            path: path("src/utilities.gfs"),
            source: &utilities_v1,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(13),
            path: path("src/isolated.gfs"),
            source: &isolated,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let roots = FrontendRoots::default();
    let mut session = FrontendSession::new();
    session.check(FrontendUpdate {
        catalog: io_catalog(),
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
        kind: FrontendModuleKind::Standard,
    }];
    let report = session.check(FrontendUpdate {
        catalog: io_catalog(),
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
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(2),
            path: path("std/iterable.gfs"),
            source: &iterable,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let roots = FrontendRoots::new(vec![SemanticRoot::new(
        SemanticRootKind::Entry,
        ModuleId::new(1),
        path("src/main.gfs"),
    )]);
    let mut session = FrontendSession::new();

    session.check(FrontendUpdate {
        catalog: io_catalog(),
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
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(2),
            path: path("std/iterable.gfs"),
            source: &iterable,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let roots = FrontendRoots::new(vec![SemanticRoot::new(
        SemanticRootKind::Entry,
        ModuleId::new(1),
        path("src/main.gfs"),
    )]);
    let mut session = FrontendSession::new();

    session.check(FrontendUpdate {
        catalog: io_catalog(),
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
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(2),
            path: path("src/utility.gfs"),
            source: &utility,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let roots = FrontendRoots::new(vec![SemanticRoot::new(
        SemanticRootKind::Entry,
        ModuleId::new(1),
        path("src/main.gfs"),
    )]);
    let mut session = FrontendSession::new();

    session.check(FrontendUpdate {
        catalog: io_catalog(),
        source_revision: Revision::new(1),
        sources: &initial_sources,
        removed_modules: &[],
        roots: &roots,
    });
    session.check(FrontendUpdate {
        catalog: io_catalog(),
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

#[test]
fn check_reports_required_builtins_in_canonical_path_order() {
    let main = SourceFile::new(
        SourceId::new(1),
        "src/main.gfs".to_string(),
        r#"
            import ansi from "format/ansi"
            import io from "std/io"
            import format from "format"
            import text from "text"

            fn main(): i32 { return 0 }
        "#
        .to_string(),
    );
    let sources = [FrontendSource {
        module_id: ModuleId::new(1),
        path: path("src/main.gfs"),
        source: &main,
        kind: FrontendModuleKind::Standard,
    }];
    let mut session = FrontendSession::new();

    let report = session.check(FrontendUpdate {
        catalog: io_catalog(),
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
    });

    let required = report
        .required_dependencies
        .iter()
        .map(|path| path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(required, ["format.gfs", "format/ansi.gfs", "text.gfs"]);
}

#[test]
fn check_exposes_semantic_modules_in_canonical_module_id_order() {
    let main = SourceFile::new(
        SourceId::new(1),
        "src/main.gfs".to_string(),
        "fn main(): i32 { return 0 }".to_string(),
    );
    let utility = SourceFile::new(
        SourceId::new(2),
        "src/utility.gfs".to_string(),
        "fn utility(): i32 { return 0 }".to_string(),
    );
    let helper = SourceFile::new(
        SourceId::new(3),
        "src/helper.gfs".to_string(),
        "fn helper(): i32 { return 0 }".to_string(),
    );
    let sources = [
        FrontendSource {
            module_id: ModuleId::new(41),
            path: path("src/main.gfs"),
            source: &main,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(7),
            path: path("src/utility.gfs"),
            source: &utility,
            kind: FrontendModuleKind::Standard,
        },
        FrontendSource {
            module_id: ModuleId::new(13),
            path: path("src/helper.gfs"),
            source: &helper,
            kind: FrontendModuleKind::Standard,
        },
    ];
    let mut session = FrontendSession::new();

    let report = session.check(FrontendUpdate {
        catalog: io_catalog(),
        source_revision: Revision::new(1),
        sources: &sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
    });

    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let module_ids = session
        .semantic_graph()
        .modules()
        .map(|module| module.id())
        .collect::<Vec<_>>();
    assert_eq!(
        module_ids,
        [ModuleId::new(7), ModuleId::new(13), ModuleId::new(41)]
    );
}

#[test]
fn snapshot_preserves_the_checked_frontend_state() {
    let initial = SourceFile::new(
        SourceId::new(1),
        "src/main.gfs".to_string(),
        "fn initial(): i32 { return 0 }".to_string(),
    );
    let initial_sources = [FrontendSource {
        module_id: ModuleId::new(7),
        path: path("src/main.gfs"),
        source: &initial,
        kind: FrontendModuleKind::Standard,
    }];
    let mut session = FrontendSession::new();
    let initial_report = session.check(FrontendUpdate {
        catalog: io_catalog(),
        source_revision: Revision::new(1),
        sources: &initial_sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
    });
    let snapshot = session.snapshot(initial_report.semantic_revision);
    assert!(Arc::ptr_eq(&snapshot.modules()[0], &session.modules()[0]));

    let updated = SourceFile::new(
        SourceId::new(1),
        "src/main.gfs".to_string(),
        "fn updated(): i32 { return 1 }".to_string(),
    );
    let updated_sources = [FrontendSource {
        module_id: ModuleId::new(7),
        path: path("src/main.gfs"),
        source: &updated,
        kind: FrontendModuleKind::Standard,
    }];
    session.check(FrontendUpdate {
        catalog: io_catalog(),
        source_revision: Revision::new(2),
        sources: &updated_sources,
        removed_modules: &[],
        roots: &FrontendRoots::default(),
    });

    let snapshot_module = snapshot
        .semantic_graph()
        .get(ModuleId::new(7))
        .expect("snapshot module");
    assert_eq!(snapshot_module.source().text(), initial.text());
    assert_eq!(snapshot.modules()[0].source().text(), initial.text());
    assert_ne!(session.modules()[0].source().text(), initial.text());
}
