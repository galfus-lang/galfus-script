# Embedding Galfus in a Rust Application

Galfus provides the `galfus-workspace` API to manage, compile, and execute Galfus source code embedded inside a Rust application.

The host application owns the source files and configuration, while `Workspace` manages incremental compilation and persistent execution states.

## Add dependencies

Add the Galfus crates to your `Cargo.toml`:

```toml
[dependencies]
galfus-workspace = { path = "../galfus-script/crates/galfus-workspace" }
galfus-contract = { path = "../galfus-script/crates/galfus-contract" }
galfus-runtime = { path = "../galfus-script/crates/galfus-runtime" }
```

## Configure and compile source with `Workspace`

Load workspace configuration and source modules, check them for semantic errors, and compile into bytecode.

```rust
use galfus_workspace::{LoadResult, Workspace};

let mut workspace = Workspace::new();

// Load workspace configuration defining module target and entry points
assert!(matches!(
    workspace.load_config(br#"
        [module]
        name = "embedded-app"
        target = "app"
        entry = "main.gfs"

        [run]
        entry = "main"
    "#)?,
    LoadResult::Success
));

// Load source modules
assert!(matches!(
    workspace.load_module("main.gfs", b"export fn main(args: [[u8]]): i32 { return 0 }")?,
    LoadResult::Success
));

// Check semantics
let checked = workspace.check();
if !checked.is_valid {
    // Present checked.diagnostics to the application
    return Err("Galfus source is invalid".into());
}

// Compile workspace
workspace.compile()?;
```

Updating source modules or configuration invalidates internal compilation state, so call `workspace.check()` and `workspace.compile()` again after any changes.

## Start and run an execution

`workspace.start_execution` resolves the entry module and entry function directly from the workspace configuration and initializes a persistent `Execution`.

```rust
use std::rc::Rc;
use galfus_runtime::CooperativeDriver;

let driver = Rc::new(CooperativeDriver::new());

// Arguments, optional providers, and driver
let mut execution = workspace.start_execution(&[], None, driver)?;

match execution.run_to_completion() {
    Ok(value) => println!("program returned {value:?}"),
    Err(failure) => eprintln!("execution failed: {failure}"),
}
```

For simple executions that do not require explicit handle management during execution, `workspace.run` is a convenience wrapper:

```rust
workspace.run(&[], None, driver)?;
```

> [!NOTE]
> `CooperativeDriver` is a minimal, **optional** native driver provided for quick setups and simple integrations. It is just one way to embed Galfus. For applications with an existing event loop or custom threading needs, you should create your own executor by implementing `KernelDriver` to schedule main-thread vs worker-thread kernel tasks yourself.

## Add host capabilities

Galfus source accesses native host capabilities through `HostProvider`. Providers process requests dispatched by Galfus code using a `MessageInjector`.

```rust
use std::sync::Arc;
use galfus_contract::{BoundaryValue, HostProvider, MessageInjector, Providers};

struct Host;

impl HostProvider for Host {
    fn dispatch(
        &mut self,
        thread_id: usize,
        request_id: u64,
        name: &str,
        _args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        if name == "answer" {
            injector.inject_system_response(
                thread_id,
                request_id,
                Ok(BoundaryValue::I32(42)),
            );
        }
    }
}

let providers = Providers::with_host(Box::new(Host));
let mut execution = workspace.start_execution(&[], Some(providers), driver)?;
```

Host providers default to main-thread affinity (`TaskAffinity::Main`). Override `HostProvider::affinity` to return `TaskAffinity::Any` only when the provider can safely run on worker executor lanes.

## Cancellation and external completion

`Execution::cancel` requests shutdown of the entire execution. Host callbacks can retain an `ExecutionHandle` to cancel threads, cancel execution, or resolve pending requests asynchronously.

```rust
let handle = execution.handle();
handle.cancel();
// Or from a host callback:
// handle.resolve_request(thread_id, request_id, Ok(BoundaryValue::Null));
```

## Error handling

Errors during loading, checking, compiling, and running are structured:

- `workspace.check()` provides diagnostic messages via `checked.diagnostics`.
- `workspace.compile()` returns `Result<CompileReport, CompileBlocked>`.
- `workspace.start_execution()` returns `Result<Execution, RunBlocked>`.
- `execution.run_to_completion()` returns `Result<BoundaryValue, ExecutionFailure>`.

Inspect the fields of `ExecutionFailure` (`kind`, IDs, `stack`, `cause`) for structured error diagnosis.
