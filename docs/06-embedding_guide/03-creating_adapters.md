# Creating and Registering Adapters

The **Adapter System** is Galfus's advanced Foreign Function Interface (FFI). It is used to bind arbitrary proxy modules (`.gfp`) to dynamic Rust code at runtime.

To expose custom Rust logic through an adapter, you need three components:

1. An `AdapterSchema` (Validates `.gfp` metadata during compilation).
2. An `AdapterModuleLoader` (Translates the required `.gfp` into a runnable binding).
3. An `AdapterModuleBinding` (The instance that actually executes the proxy calls).

## 1. The Schema (Compile-Time Validation)

The schema defines what keys are allowed in the `.gfp` file's configuration block.

```rust
use galfus_contract::{AdapterSchema, AdapterConfig};

pub struct DatabaseAdapterSchema;

impl AdapterSchema for DatabaseAdapterSchema {
    fn name(&self) -> &str {
        "db_adapter"
    }

    fn validate(&self, _config: &AdapterConfig) -> Result<(), String> {
        // You could validate if `config` contains a "table_name" key here.
        Ok(())
    }
}
```

## 2. The Binding (Runtime Execution)

The binding is what stays alive during execution. It receives the method names and arguments defined by the proxy module.

```rust
use std::sync::Arc;
use galfus_contract::{AdapterModuleBinding, BoundaryValue, MessageInjector, ExecutionFailure};

pub struct DatabaseBinding {
    table_name: String,
}

impl AdapterModuleBinding for DatabaseBinding {
    fn descriptor(&self) -> galfus_contract::AdapterModuleDescriptor {
        galfus_contract::AdapterModuleDescriptor {
            module_path: "db_adapter".into(),
            schema_fingerprint: 0,
            boundary_abi: galfus_contract::CURRENT_BOUNDARY_ABI_VERSION,
            exports: vec![],
        }
    }

    fn dispatch(
        &mut self,
        symbol: &str,
        thread_id: galfus_core::ThreadId,
        request_lease: galfus_core::RequestLease,
        args: &[BoundaryValue],
        injector: Arc<dyn MessageInjector>,
    ) {
        if symbol == "fetch_user" {
            println!("Fetching user from table: {}", self.table_name);

            // Return dummy data
            let _ = injector.inject_system_response(thread_id, request_lease, Ok(BoundaryValue::I64(42)));
        } else {
            let _ = injector.inject_system_response(
                thread_id,
                request_lease,
                Err(ExecutionFailure::AdapterError("Method not found".into()))
            );
        }
    }

    fn cancel(
        &mut self,
        _symbol: &str,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
    ) -> galfus_contract::CancellationOutcome {
        galfus_contract::CancellationOutcome::Unsupported
    }
}
```

## 3. The Loader (The Bridge)

The loader is invoked right before execution begins. It reads the requirements from the workspace and yields the binding.

```rust
use galfus_contract::{
    AdapterModuleLoader, AdapterModuleRequirement, AdapterLoadContext, AdapterLoadError, AdapterModuleBinding
};

pub struct DatabaseAdapterLoader;

impl AdapterModuleLoader for DatabaseAdapterLoader {
    fn name(&self) -> &str {
        "db_adapter"
    }

    fn load(
        &self,
        requirement: &AdapterModuleRequirement,
        _context: &AdapterLoadContext,
    ) -> Result<Box<dyn AdapterModuleBinding>, AdapterLoadError> {

        // Extract configuration from the .gfp file
        let table = match requirement.config.get("table") {
            Some(galfus_contract::AdapterConfigValue::String(s)) => s.clone(),
            _ => "default_table".to_string(),
        };

        Ok(Box::new(DatabaseBinding { table_name: table }))
    }
}
```

## Registration

To use these components, register the schema in the `WorkspaceConfig` so the compiler recognizes it, and register the loader in the execution phase.

```rust
use galfus_workspace::{Workspace, WorkspaceConfig};
use std::rc::Rc;

// 1. Register schema for compilation
let mut config = WorkspaceConfig::new();
config.with_adapter_schema(Box::new(DatabaseAdapterSchema));

let mut workspace = Workspace::new(config);
// ... load source files ...

// 2. Register loaders for execution
let mut loaders = galfus_contract::AdapterLoaders::new();
loaders.register(Box::new(DatabaseAdapterLoader));

// 3. Execute
let _ = workspace.run_with_adapters(&[], None, loaders, driver);
```
