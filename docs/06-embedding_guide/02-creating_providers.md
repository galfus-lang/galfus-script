# Creating and Registering Providers

In Galfus, **Providers** are the mechanism used to expose global, standard-library-like functionalities (such as `std::io` or `std::time`) to the language environment without compromising the core sandbox.

Unlike adapters, which are loaded via abstract `.gfp` proxy modules, providers satisfy concrete declarative requirements mapped in the `CapabilityCatalog`.

## Implementing `HostProvider`

To build a custom provider, you must implement the `galfus_contract::HostProvider` trait.

This trait allows you to intercept dispatches from the Galfus runtime. When a script calls a provider function, your Rust code takes over.

```rust
use std::sync::Arc;
use galfus_contract::{BoundaryValue, HostProvider, MessageInjector, TaskAffinity, CancellationOutcome};

pub struct MyIOProvider;

impl HostProvider for MyIOProvider {
    /// Define the thread affinity. `TaskAffinity::Main` is the default and safest
    /// for operations touching OS handles (like stdout).
    fn affinity(&self, _name: &str) -> TaskAffinity {
        TaskAffinity::Main
    }

    /// Handles an incoming call from a Galfus script.
    fn dispatch(
        &mut self,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        name: &str,
        args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        match name {
            "__provider_io_print" => {
                // Parse arguments
                if let Some(BoundaryValue::Array { bytes: Some(bytes), .. }) = args.get(0) {
                    if let Ok(text) = std::str::from_utf8(bytes) {
                        println!("{}", text);
                    }
                }

                // Complete the activation synchronously.
                let _ = injector.inject_system_response(thread_id, request_lease, Ok(BoundaryValue::Null));
            },
            _ => {
                // Reject unknown functions
                let _ = injector.inject_system_response(
                    thread_id,
                    request_lease,
                    Err(galfus_contract::ExecutionFailure::ProviderError("Unknown function".into()))
                );
            }
        }
    }

    /// Handle task cancellations if the Galfus runtime unmounts or drops the owning thread.
    fn cancel(&mut self, _thread_id: galfus_core::ThreadId, _request_lease: galfus_core::RequestLease) -> CancellationOutcome {
        CancellationOutcome::Unsupported
    }
}
```

## Registering the Provider

Once your provider is built, you must pass it to the `Workspace` at execution time using the `Providers` struct.

```rust
use galfus_workspace::Workspace;
use galfus_contract::Providers;
use galfus_runtime::CooperativeDriver;
use std::rc::Rc;

fn run_with_custom_provider(workspace: &mut Workspace) {
    let mut providers = Providers::new();

    // Inject our custom IO Provider as the primary Host provider
    providers = Providers::with_host(Box::new(MyIOProvider));

    let driver = Rc::new(CooperativeDriver::new());

    // Execute the workspace with the injected providers
    let _ = workspace.run(&[], Some(providers), driver);
}
```

### Async Operations

If your provider performs a heavy operation (like a network request), you do not need to block the `dispatch` function. You can spawn a Tokio task, move an `Arc` clone of the `injector` inside, and call `inject_system_response` asynchronously whenever the data is ready. The Galfus runtime will keep the caller `VirtualThread` suspended until the injection occurs.
