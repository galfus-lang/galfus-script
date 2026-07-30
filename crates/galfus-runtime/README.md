# galfus-runtime

`galfus-runtime` validates entrypoints and orchestrates persistent executions
over an `Arc<BytecodeGraph>`.

## Responsibilities

- **Entrypoint Execution**: Validates and invokes exported module entries.
- **Persistent Execution**: `Runtime::start` returns an `Execution` that hosts
  can poll, cancel, and inspect after completion.
- **Host Integration**: Receives `Providers` from an embedding host or workspace and routes capability requests to the host platform.

The runtime does not rebuild or duplicate the `BytecodeGraph`. VM state is
per virtual thread and includes module initialization/global state, heap, and
call frames. Providers and adapters are optional host-owned registries.

`ExecutionFailure` preserves machine-readable categories, IDs, causes, and VM
frames. Optional `ExecutionMetadata` can map instruction offsets to source
spans, but source locations are not yet exposed directly by the failure type.
