# galfus-contract

`galfus-contract` defines the optional host integration contracts used by a Galfus
execution. It contains no target selection and no concrete platform adapter.

## Responsibilities

- **Providers**: Owns the optional providers supplied for one execution.
- **HostProvider**: Defines an asynchronous, message-based dispatch contract for executing native host capabilities.
- **BoundaryValue**: Typed values crossing the VM/host boundary.
- **MessageInjector**: Trait for injecting responses back into a suspended virtual thread.
- **Adapters**: Optional nominal adapter registry with affinity, cancellation,
  and external-handle release hooks.

Hosts construct `Providers` and pass them to `Runtime::new` or
`Workspace::start_execution`. If no host provider is supplied, only executions
that reach native calls fail at runtime; compilation and executions without
native calls remain valid.
