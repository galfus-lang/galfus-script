# Why Galfus Is Embeddable

Galfus is designed to be usable as a component inside a Rust application, not
only as a standalone command-line language. Its current embedding strengths are
practical boundaries in the codebase rather than claims of universal platform
support.

## In-memory executable graph

Compilation produces a validated `BytecodeGraph` held in memory. The runtime
executes that graph directly and does not parse source, rebuild the graph, or
write a bytecode bundle as part of execution. A host can therefore choose where
source loading, compilation, caching, and execution occur.

## Separate source and execution APIs

`galfus-workspace` is useful when a host manages source files and wants the
standard check/compile lifecycle. `galfus-runtime` starts an `Execution` from
an existing graph. This split lets applications use only the layer they need.

## Explicit host boundary

Native calls use `HostProvider` and typed `BoundaryValue` values. The provider
gets a thread ID, request ID, arguments, and a `MessageInjector`; it does not
receive mutable access to VM or scheduler internals. This makes immediate and
callback-based host operations possible while preserving orchestrator ownership
of execution state.

The absence of a provider is also explicit. Programs without native calls can
run without one. A reached native call fails with a structured missing-provider
error rather than silently gaining host access.

## Host-controlled scheduling

Execution is persistent and can be polled. `KernelDriver` separates work that
must execute on the host main thread from transferable work. This is useful for
applications with UI/event loops, game loops, or custom worker executors. The
provided cooperative driver is intentionally simple; it is not a substitute for
an application's scheduler.

## Structured failures and cancellation

Execution failures carry a category, optional runtime IDs, a causal failure,
and asynchronous VM frames. Cancellation requests go through the orchestrator,
which removes runnable and timed waiting state, notifies pending providers and
adapters, and ignores late completions safely. These properties make embedding
code able to make decisions from data rather than formatted error strings.

## Isolated execution state

Virtual threads own private heaps. Inter-thread mailboxes carry bytes, not host
pointers or arbitrary structured values. The VM's ownership graph releases
unreachable values deterministically and handles cycles without a global
garbage collector. These choices reduce the amount of host state that crosses
an execution boundary.

## Current limits

Embeddable does not mean complete or sandboxed by default. In particular:

- There is no stable packaged-crate release documented by this repository yet.
- Adapter calls and future waits have runtime preparation, but are not emitted
  by the Galfus compiler from source.
- The adapter descriptor, dynamic payload loading, capability-policy, and
  distribution material described in older design documents are proposals, not
  current runtime features.
- The host remains responsible for choosing providers, scheduling policy,
  resource limits, and trust boundaries.

Those constraints are deliberate to state plainly: Galfus is already a useful
Rust-embeddable VM pipeline, while several broader interoperability and async
language features remain future work.
