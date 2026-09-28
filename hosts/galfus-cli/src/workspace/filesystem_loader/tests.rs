use super::*;

use galfus_workspace::{SourceLoadErrorKind, WorkspaceSourceLoader};
use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ROOT_ID: AtomicUsize = AtomicUsize::new(0);

fn temporary_root(label: &str) -> PathBuf {
    env::current_dir()
        .expect("current directory")
        .join(".tmp")
        .join(format!(
            "filesystem-source-loader-{label}-{}",
            NEXT_ROOT_ID.fetch_add(1, Ordering::Relaxed)
        ))
}

#[test]
fn loader_reads_only_the_requested_nested_module() {
    let root = temporary_root("nested");
    fs::create_dir_all(root.join("src/nested")).expect("workspace directory");
    fs::write(
        root.join("src/nested/helper.gfs"),
        "export fn helper(): i32 { return 1 }",
    )
    .expect("nested source");
    fs::write(
        root.join("src/unrequested.gfs"),
        "invalid source that must not be loaded",
    )
    .expect("unrequested source");
    let loader = FilesystemSourceLoader::new(root.as_path()).expect("loader initializes");
    let module_path = ModulePath::new("src/nested/helper.gfs").expect("valid module path");

    let source = loader
        .load_source(&module_path)
        .expect("requested source loads");

    assert_eq!(
        source.bytes().as_ref(),
        b"export fn helper(): i32 { return 1 }"
    );
    fs::remove_dir_all(root).expect("temporary workspace removed");
}

#[test]
fn loader_reports_missing_module_with_its_canonical_path() {
    let root = temporary_root("missing");
    fs::create_dir_all(&root).expect("workspace directory");
    let loader = FilesystemSourceLoader::new(root.as_path()).expect("loader initializes");
    let module_path = ModulePath::new("src/missing.gfs").expect("valid module path");

    let error = loader
        .load_source(&module_path)
        .expect_err("missing source is explicit");

    assert_eq!(error.requested_path(), &module_path);
    assert_eq!(error.kind(), &SourceLoadErrorKind::NotFound);
    fs::remove_dir_all(root).expect("temporary workspace removed");
}

#[cfg(unix)]
#[test]
fn loader_rejects_a_symlink_that_escapes_the_workspace_root() {
    use std::os::unix::fs::symlink;

    let root = temporary_root("escape");
    let outside = root.with_extension("outside.gfs");
    fs::create_dir_all(&root).expect("workspace directory");
    fs::write(&outside, "export fn outside(): i32 { return 0 }").expect("outside source");
    symlink(&outside, root.join("escape.gfs")).expect("escape symlink");
    let loader = FilesystemSourceLoader::new(root.as_path()).expect("loader initializes");
    let module_path = ModulePath::new("escape.gfs").expect("valid module path");

    let error = loader
        .load_source(&module_path)
        .expect_err("escaping symlink is rejected");

    assert_eq!(error.requested_path(), &module_path);
    assert_eq!(error.kind(), &SourceLoadErrorKind::PathEscapesWorkspace);
    fs::remove_dir_all(root).expect("temporary workspace removed");
    fs::remove_file(outside).expect("outside source removed");
}
