# Capability Catalog

`CapabilityCatalog` is the declarative capability set available to one
`Workspace`. It authorizes provider bridge modules and adapter schemas during
semantic checking and compilation. It does not contain native implementations
and it is not the runtime provider registry.

The split is deliberate:

| Concern | Catalog | Execution capabilities |
| --- | --- | --- |
| Provider bridge source | `BridgeModule` | No |
| Adapter schema | `AdapterSchema` | No |
| Native provider implementation | No | `Providers` |
| Adapter binding implementation | No | `AdapterBindings` |
| When it is used | Check and compile | Runtime startup and execution |

For example, a workspace may know the interface of `std/fs` because it is in
the catalog, but it can only execute a package importing `std/fs` when the host
also installs a compatible `fs` provider.

## Build a catalog

Create the catalog from provider bridge sources and optional adapter schemas.
Provider paths are logical module paths, not filesystem paths.

```rust
use std::sync::Arc;

use galfus_contract::{BridgeModule, CapabilityCatalog};
use galfus_workspace::Workspace;

let catalog = CapabilityCatalog::new(
    vec![BridgeModule::new(
        "vendor/io",
        br#"
        fn(async) __provider_vendorio_write(text: [u8]): null

        export fn write(text: [u8]): null {
          await __provider_vendorio_write(text)
        }
        "#,
    )],
    Vec::new(),
)?;

let mut workspace = Workspace::new();
workspace.set_catalog(Arc::new(catalog));
```

The host must set the catalog before checking or compiling modules that import
its provider bridges. The native host uses `native_catalog()` to register the
standard `std/io`, `std/env`, `std/time`, `std/fs`, `std/net`, `std/http`, and
`std/websocket` bridges. The web host catalog exposes `std/io`, `std/env`,
`std/time`, `std/http`, and `std/websocket`.

## Provider bridge entries

A `BridgeModule` pairs a module path with its declarative `.gfs` source.

```rust
BridgeModule::new("vendor/io", VENDOR_IO_BRIDGE)
```

The catalog rejects provider paths that are empty, contain `\`, NUL, `.`, or
`..` segments. It also rejects duplicate paths and paths colliding with engine
internal builtins such as `std/async`.

The bridge source is compiled as a catalog-owned module. User source cannot
replace it by loading a module at the same path.

Provider calls in the bridge must use the alias convention described in
[Creating and Registering Providers](./02-creating_providers.md):

```galfus
fn(async) __provider_vendorio_write(text: [u8]): null
```

The catalog authorizes the module path; the operation's `vendorio` alias later
selects the concrete provider in `Providers`.

## Adapter schema entries

Adapter schemas are registered by adapter name and validate `.gfp` module
descriptors. They do not provide a bridge source and they do not create an
adapter binding.

```rust
use std::sync::Arc;
use galfus_contract::{AdapterModuleDescriptor, AdapterSchema, AdapterValidationError};

struct DemoSchema;

impl AdapterSchema for DemoSchema {
    fn name(&self) -> &str {
        "demo"
    }

    fn catalog_schema(&self) -> String {
        "demo-v1".to_string()
    }

    fn validate_schema(
        &self,
        descriptor: &AdapterModuleDescriptor,
    ) -> Result<(), AdapterValidationError> {
        let _ = descriptor;
        Ok(())
    }
}

let catalog = CapabilityCatalog::new(
    Vec::new(),
    vec![Arc::new(DemoSchema)],
)?;
```

The catalog rejects duplicate adapter names. At runtime, the host must still
provide the corresponding adapter loader and binding for a package that uses
the schema.

## Changes and invalidation

The catalog has a deterministic fingerprint based on provider paths and source
contents plus each adapter schema's `catalog_schema()` value. Calling
`Workspace::set_catalog` with a different fingerprint:

1. increments the workspace source revision;
2. removes modules previously loaded from the provider catalog;
3. marks the workspace dirty;
4. causes the next `check()` to load the new catalog sources.

Changing a provider bridge source therefore requires `check()` and `compile()`
again. A package compiled from the new source carries a new provider schema
fingerprint, so runtime preflight rejects hosts that still expose the old
descriptor.

Calling `set_catalog` with a catalog that has the same fingerprint leaves the
workspace unchanged.

## From catalog to execution

The complete flow is:

```text
CapabilityCatalog
  └─ authorizes bridge source and adapter schemas in Workspace
       └─ Workspace check/compile records provider and adapter requirements
            └─ Runtime startup preflight validates host capabilities
                 └─ Providers route __provider_<alias>_<operation> by alias
```

The catalog alone never grants host access. Conversely, registering a native
provider without cataloguing its bridge does not make an importable module
available to a workspace. Both sides are required.
