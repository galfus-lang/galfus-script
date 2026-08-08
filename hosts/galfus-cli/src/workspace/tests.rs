use std::env;
use std::fs;
use std::sync;

use super::*;
use galfus_runtime::CooperativeDriver;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_WORKSPACE_ID: AtomicUsize = AtomicUsize::new(0);

#[test]
fn load_workspace_reads_all_nested_source_files() {
    let workspace_root = env::current_dir()
        .expect("current directory")
        .join(".tmp")
        .join(format!(
            "runner-load-workspace-{}",
            NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(workspace_root.join("nested")).expect("temporary workspace");
    fs::write(
        workspace_root.join("galfus.toml"),
        "[module]\nname = \"runner-test\"\ntarget = \"app\"\nentry = \"main.gfs\"\n",
    )
    .expect("configuration");
    fs::write(
        workspace_root.join("main.gfs"),
        "import { helper } from \"./nested/helper.gfs\"\nexport fn main(args: [[u8]]): i32 { return helper() }",
    )
    .expect("entry source");
    fs::write(
        workspace_root.join("nested/helper.gfs"),
        "export fn helper(): i32 { return 1 }",
    )
    .expect("nested source");

    let mut workspace = load_workspace(workspace_root.as_path()).expect("loads workspace");
    assert!(workspace.check().is_valid);
    assert_eq!(
        workspace
            .compile()
            .expect("compiles workspace")
            .package
            .graph()
            .len(),
        2
    );

    fs::remove_dir_all(workspace_root).expect("remove temporary workspace");
}

#[test]
fn load_workspace_accepts_standalone_source_file() {
    let source_path = env::current_dir()
        .expect("current directory")
        .join(".tmp")
        .join(format!(
            "runner-standalone-source-{}.gfs",
            NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(source_path.parent().expect("temporary directory"))
        .expect("temporary directory");
    fs::write(
        source_path.as_path(),
        "export fn main(args: [[u8]]): i32 { return 42 }",
    )
    .expect("entry source");

    let mut workspace = load_workspace(source_path.as_path()).expect("loads standalone source");
    assert!(workspace.check().is_valid);
    workspace.compile().expect("compiles standalone source");
    use galfus_contract::KernelDriver;
    let executor = std::rc::Rc::new(CooperativeDriver::new());
    let exit_code = sync::Arc::new(sync::Mutex::new(0));
    let ec = sync::Arc::clone(&exit_code);
    executor.on_exit(Box::new(move |res| {
        *ec.lock().unwrap() = res.unwrap();
    }));
    workspace
        .run(&[], None, executor)
        .expect("runs standalone source");
    assert_eq!(*exit_code.lock().unwrap(), 42);

    fs::remove_file(source_path).expect("remove temporary source");
}

#[test]
fn run_project_returns_the_application_exit_code() {
    let source_path = env::current_dir()
        .expect("current directory")
        .join(".tmp")
        .join(format!(
            "runner-project-exit-code-{}.gfs",
            NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(source_path.parent().expect("temporary directory"))
        .expect("temporary directory");
    fs::write(
        source_path.as_path(),
        "export fn main(args: [[u8]]): i32 { return 42 }",
    )
    .expect("entry source");

    assert_eq!(
        run_project(source_path.to_str().expect("UTF-8 source path"), &[])
            .expect("runs standalone source"),
        42
    );

    fs::remove_file(source_path).expect("remove temporary source");
}

#[test]
fn run_project_spawns_a_thread_with_the_anchored_api() {
    let source_path = env::current_dir()
        .expect("current directory")
        .join(".tmp")
        .join(format!(
            "runner-thread-api-{}.gfs",
            NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed)
        ));
    fs::write(
        source_path.as_path(),
        r#"
            import { createThread, getThread } from 'std/thread'

            fn worker(args: [[u8]]): i32 {
                return 0
            }

            export fn main(args: [[u8]]): i32 {
                const thread = createThread(worker, "worker")
                if getThread("worker") == null {
                    return 1
                }
                if !thread::spawn() {
                    return 2
                }
                if !thread::isRunning() {
                    return 3
                }
                if thread::isExited() {
                    return 4
                }
                return 0
            }
        "#,
    )
    .expect("entry source");

    assert_eq!(
        run_project(source_path.to_str().expect("UTF-8 source path"), &[])
            .expect("runs thread API"),
        0
    );

    fs::remove_file(source_path).expect("remove temporary source");
}
