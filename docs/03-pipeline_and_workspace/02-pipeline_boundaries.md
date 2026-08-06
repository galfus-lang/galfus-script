# Galfus Pipeline Boundaries

`galfus-workspace` is the stateful facade used by hosts. A host loads the
configuration and source bytes, then calls `check`, `compile`, and `run` in
that order. Hosts do not compose frontend, compiler, or runtime state.

## Source boundary

`SourceStore` belongs to `galfus-workspace`. It stores source bytes by
`ModulePath`, assigns a stable `ModuleId` on first load, and records a
`Revision` for every update. Replacing bytes for an existing path preserves
the module ID. Removing a source removes its entry and reports that stable ID
to the frontend delta.

## Frontend boundary

`FrontendSession` consumes `FrontendUpdate`: changed `FrontendSource` values,
removed module IDs, source revision, and roots. It owns the
`SemanticModuleGraph`, whose nodes are `SemanticModule` values keyed by
`ModuleId`; every node carries a `SemanticRevision`. Its report contains
diagnostics, required builtin module paths, and the set of module IDs whose
semantic output changed. The compiler must consume the semantic model rather
than source text or syntax-only output.

## Compiler boundary

The compiler consumes semantic modules and produces a versioned
`BytecodeGraphTransaction` containing changed modules, removals, and dependency
edges. The workspace applies it only when its base version matches, validates
the complete resulting graph, and publishes the next snapshot atomically.

The `BytecodeGraph` is the single canonical executable graph.

## Runtime boundary

The runtime executes a borrowed `BytecodeGraph` directly. The VM currently
holds execution state, including heap and module-scoped globals, and does not
rebuild, copy, or duplicate the graph. Dependencies initialize before the
entry module.
The runtime never performs parsing, semantic checking, or compilation, and there is no separate runtime module graph.

## Host-provider boundary

`galfus-contract` defines optional host contracts independently of the workspace,
runtime, and VM. A host constructs `Providers` with concrete implementations
and composes `Runtime::new(graph, providers)` directly, or passes them to
`Workspace::start_execution` when source management is needed. The workspace remains the
development facade and does not expose its internal runtime or VM state.

Providers are execution-scoped. The compiler does not inspect or validate
them. The runtime passes them to the VM, which reports a runtime error only if
an instruction requires a missing provider. Consequently, running without
providers is a valid sandbox configuration for programs that do not reach
host-backed builtins.

The current `HostProvider` supports asynchronous payload dispatching
through message injection. It is intentionally independent of native, WASM, or
browser APIs: the CLI adapts native OS streams, and the playground adapts its
buffered streams to JavaScript through its WASM-facing API, all running on top of
a suspended virtual thread.

## Gate rules

`Workspace::compile` requires a successful, current `Workspace::check`.
`Workspace::run` requires a successful, current `Workspace::compile`. A
source or configuration update invalidates the later stages until the pipeline
is run again.
