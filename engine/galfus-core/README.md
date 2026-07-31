# galfus-core

`galfus-core` provides the shared foundational primitives and utility types for the Galfus compiler and runtime.

## Responsibilities

- **Diagnostics**: Error and warning reporting representation (`Diagnostic`, `Severity`).
- **Span Management**: Location tracking in source files (`Span`).
- **Shared IDs**: Identifiers such as `FunctionId`, `SymbolId`, `TypeId`, and `NodeId`.
- **Primitive Metadata**: Definitions of primitive types and basic type tags.
