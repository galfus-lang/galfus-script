# Creating and Registering Providers

A provider exposes a host capability to Galfus. It has two parts that must be
kept in sync:

1. a declarative bridge module (`.gfs`) registered in the `CapabilityCatalog`;
2. a Rust `HostProvider` registered in `Providers` for an execution.

The bridge is what a script imports. The provider is what performs the native
operation. A compiled package records the exact bridge surface it used, and the
runtime validates that surface against the provider before execution starts.

## Bridge operations and aliases

Every native bridge operation uses this form:

```galfus
fn(async) __provider_<alias>_<operation>(...): ReturnType
```

For example, a module at `vendor/io` may use the alias `vendorio`:

```galfus
fn(async) __provider_vendorio_write(text: [u8]): null

export fn write(text: [u8]): null {
  await __provider_vendorio_write(text)
}
```

The module path does **not** select the provider. The runtime extracts the
alias from the operation name and routes `__provider_vendorio_write` to the
provider registered as `vendorio`.

Aliases must contain only lowercase ASCII letters and digits. In particular,
`vendor_io` is invalid because an underscore separates the alias from the
operation.

The `HostProvider::dispatch` method receives the operation after the
`__provider_` prefix has been removed. The example above therefore arrives as
`vendorio_write`, not `__provider_vendorio_write`.

## Implement `HostProvider`

The provider descriptor is an immutable description of every bridge module and
operation implemented by the host. It is not inferred at runtime: the runtime
does not parse bridge source while it is executing. This explicit declaration
is what makes preflight validation detect mismatched function signatures,
return types, ABI versions, or bridge source revisions before a native call is
made.

The built-in bridges have helpers such as `std_io_provider_descriptor()`. A
custom provider declares an equivalent descriptor for its own bridge.

```rust
use std::sync::Arc;

use galfus_contract::{
    provider_schema_fingerprint, BoundaryType, BoundaryValue,
    CancellationOutcome, CURRENT_BOUNDARY_ABI_VERSION, ExecutionFailure,
    ExecutionFailureKind, HostProvider, MessageInjector, ProviderDescriptor,
    ProviderFunctionSignature, ProviderModuleDescriptor, TaskAffinity,
};

const VENDOR_IO_BRIDGE: &str = r#"
fn(async) __provider_vendorio_write(text: [u8]): null

export fn write(text: [u8]): null {
  await __provider_vendorio_write(text)
}
"#;

struct VendorIoProvider;

impl HostProvider for VendorIoProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            modules: vec![ProviderModuleDescriptor {
                module_path: "vendor/io".to_string(),
                schema_fingerprint: provider_schema_fingerprint(VENDOR_IO_BRIDGE),
                boundary_abi: CURRENT_BOUNDARY_ABI_VERSION,
                exports: vec![ProviderFunctionSignature {
                    name: "vendorio_write".to_string(),
                    parameter_types: vec![BoundaryType::Array(Box::new(BoundaryType::U8))],
                    return_type: BoundaryType::Null,
                }],
            }],
        }
    }

    fn affinity(&self, _name: &str) -> TaskAffinity {
        TaskAffinity::Main
    }

    fn dispatch(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        let result = match (name, args) {
            ("vendorio_write", [BoundaryValue::Bytes(text)]) => {
                // Call the host API here. This example deliberately avoids
                // assuming that boundary bytes are valid UTF-8.
                let _ = text;
                Ok(BoundaryValue::Null)
            }
            ("vendorio_write", _) => Err(ExecutionFailure::new(
                ExecutionFailureKind::ProviderFailure,
                "vendorio_write expects one byte array",
            )),
            _ => Err(ExecutionFailure::new(
                ExecutionFailureKind::ProviderFailure,
                format!("provider operation '{name}' is not implemented"),
            )),
        };

        let _ = injector.inject_system_response(thread_id, request_lease, result);
    }

    fn cancel(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
    ) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }
}
```

`TaskAffinity::Main` is the safe default. Return `TaskAffinity::Any` only when
the complete `dispatch` implementation and the APIs it calls are safe to run on
a worker lane.

## Register the bridge and the provider

Register the bridge source in the catalog used by the workspace, then register
the corresponding host under the same alias used in its declarations:

```rust
use galfus_contract::{BridgeModule, CapabilityCatalog, Providers};

let catalog = CapabilityCatalog::new(
    vec![BridgeModule::new("vendor/io", VENDOR_IO_BRIDGE)],
    Vec::new(),
)?;

let providers = Providers::new()
    .with_host("vendorio", Box::new(VendorIoProvider));
```

`Providers` is a map, so one execution can register independent capabilities:

```rust
let providers = Providers::new()
    .with_host("io", Box::new(NativeIoProvider))
    .with_host("vendorio", Box::new(VendorIoProvider));
```

The native CLI follows this pattern through `default_providers`, registering
the `io`, `env`, `time`, `fs`, `net`, `http`, and `websocket` providers before
constructing an `ExecutionHost`.

## Completion, asynchronous work, and cancellation

For every dispatched request, a provider must eventually call
`inject_system_response` with either a `BoundaryValue` matching the descriptor
or an `ExecutionFailure`. It may do so synchronously, as in the example, or
retain the injector and complete from its own worker or event-loop callback.

Do not mutate VM state from provider code. The injector is the only path back
to the suspended virtual thread.

When an execution is cancelled, the runtime calls `HostProvider::cancel` for a
pending provider request. Return `Confirmed` when cancellation was completed,
`BestEffort` when requested but not guaranteed, `AlreadyCompleted` when no work
remains, or `Unsupported` only when the native operation cannot be cancelled.

## Why descriptors are explicit

`ProviderDescriptor` currently duplicates the typed bridge surface by design.
It lets the host validate a package without bundling the compiler or parsing
`.gfs` source at runtime. A build-time generator could remove this duplication
in the future, but generated output must still become the descriptor supplied
by `HostProvider::descriptor`.
