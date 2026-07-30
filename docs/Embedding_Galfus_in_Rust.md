# Embedding Galfus in a Rust Application

Galfus can be embedded at two levels:

- `galfus-workspace` compiles Galfus source managed by the host.
- `galfus-runtime` executes an existing `BytecodeGraph`.

Use the workspace API when the host owns source files. Use the runtime API when
another part of the application already creates and validates bytecode.

## Add dependencies

For source-based embedding, depend on the workspace and contract crates from
the same Galfus revision:

```toml
[dependencies]
galfus-workspace = { path = "../galfus-script/crates/galfus-workspace" }
galfus-contract = { path = "../galfus-script/crates/galfus-contract" }
galfus-runtime = { path = "../galfus-script/crates/galfus-runtime" }
```

## Compile source with `Workspace`

Load configuration and modules, then check and compile them. A source or
configuration update invalidates later stages, so check and compile again after
each change.

```rust
use galfus_workspace::{LoadResult, Workspace};

let mut workspace = Workspace::new();
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
assert!(matches!(
    workspace.load_module("main.gfs", b"export fn main(args: [[u8]]): i32 { 0 }")?,
    LoadResult::Success
));

let checked = workspace.check();
if !checked.is_valid {
    // Present checked.diagnostics to the application.
    return Err("Galfus source is invalid".into());
}

let compiled = workspace.compile()?;
let graph = compiled.graph;
```

The exact source syntax and configuration fields should follow the examples in
this repository. The important integration boundary is the resulting
`Arc<BytecodeGraph>`; the runtime does not parse or compile source.

## Run an execution

`Runtime::start` creates a persistent `Execution`. The host controls polling,
timeouts, cancellation, and the driver used to run kernel tasks.

```rust
use std::rc::Rc;

use galfus_runtime::{CooperativeDriver, Runtime};

let driver = Rc::new(CooperativeDriver::new());
let mut execution = Runtime::new(graph, None)
    .start(entry_module_id, "main", &[], driver)?;

match execution.run_to_completion() {
    Ok(value) => println!("program returned {value:?}"),
    Err(failure) => eprintln!("execution failed: {failure}"),
}
```

`CooperativeDriver` is a small native driver suitable for simple integrations.
Applications with an existing event loop should implement `KernelDriver` and
schedule `KernelTask::Main` on the host main thread and `KernelTask::Any` on a
compatible worker executor. Main-affine tasks are intentionally not `Send`.

## Add host capabilities

Galfus source reaches host functionality through `HostProvider`. The provider
can complete immediately or retain the injector and complete later. It must not
mutate runtime state directly; use the supplied injector instead.

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
let runtime = Runtime::new(graph, Some(providers));
```

Providers default to main-thread affinity. Return `TaskAffinity::Any` only when
the provider can safely run on the driver's worker lane. If no provider is
configured, programs that do not make native calls can still run; a reached
native call fails with `ExecutionFailureKind::MissingProvider`.

## Cancellation and external completion

`Execution::cancel` requests shutdown of the entire execution. An
`ExecutionHandle` can be retained by host callbacks to cancel a thread, cancel
the execution, or resolve a pending request/future. Completion after a request
has been cancelled is ignored by the orchestrator.

```rust
let handle = execution.handle();
handle.cancel();
// Or, from a host callback:
// handle.resolve_request(thread_id, request_id, Ok(BoundaryValue::Null));
```

The future completion APIs and `AwaitFuture` bytecode are runtime preparation
for future asynchronous language support. The Galfus compiler does not yet emit
that instruction from source.

## Adapters and handles

`Adapters` is an optional registry for typed host adapters. An adapter declares
its affinity, dispatches calls through `MessageInjector`, can observe
cancellation, and may own nominal external handles. The current compiler does
not emit adapter calls from Galfus source, so this API is intended for hosts
that construct compatible bytecode directly or are preparing an integration.

## Error handling

`ExecutionFailure` is structured. Inspect `kind`, IDs, `stack`, and `cause`
instead of parsing its display text. The VM and runtime preserve asynchronous
call frames where they are available. Source spans remain optional bytecode
metadata and are not currently exposed as a field on `ExecutionFailure`.
