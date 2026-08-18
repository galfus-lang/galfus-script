![Galfus](/assets/brand-full-colored.svg)

# Galfus Script

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE.md)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)

> A typed scripting language with an in-memory bytecode graph and a Rust-hosted VM runtime.

Galfus Script is a VM-first language project. Its current implementation covers
source checking, bytecode graph construction, and execution through the Rust
runtime. Some documented interop, packaging, and async language features remain
planned rather than source-level features.

---

## Table of Contents

- [Embedding Galfus](docs/06-embedding_guide/01-embedding_in_rust.md)
- [Why Galfus Is Embeddable](docs/01-introduction/01-why_galfus_is_embeddable.md)
- [Status](#status)
- [Core Features](#core-features)
- [Memory Philosophy](#memory-philosophy)
- [Repository Layout](#repository-layout)
- [Virtual Standard Library](#virtual-standard-library)
- [Running and Testing Locally](#running-and-testing-locally)
- [Design Goals](#design-goals)
- [Name Inspiration](#name-inspiration)
- [License](#license)

---

## Status

You can parse, typecheck, compile, and run Galfus Script projects using the
local VM runner. Embedding applications can use `galfus-workspace` for source
management or `galfus-runtime` for an existing bytecode graph.

```txt
.gfs Source Files (Workspace)
  └── Lexer & AST Parser
        └── Resolver (Scope & Name Resolution)
              └── Type Checker & Semantic Analyzer
                    └── Ownership Check
                          └── MIR Lowering (Structured IR)
                                └── Bytecode Emitter
                                       └── BytecodeGraph (in-memory)
                                                   └── VM Interpreter Execution
```

---

## Core Features

Galfus Script implements a robust set of modern language features:

- **Type Safety**: Fully typed syntax with static type inference, validation of assignments, function calls, member accesses, and expression statements.
- **Encapsulated Builtins**: Strictly prevents user projects from referencing or declaring `__provider_*`, `__internal_*`, or `__builtin_*` intrinsics directly. These are visible only inside compiler-trusted builtin scopes.
- **Structs**: Rich struct declarations supporting inline initialization, member field access, and typed layouts.
- **Dynamic Array Spreads**: Array literal spread operators (`[...arr1, ...arr2]`) computed dynamically at runtime using custom `Len` and `CopyArray` VM instructions.
- **Control Flow**: Conditionals (`if`/`else`), loop jumps, and comparison operators.
- **Workspace Linking**: Cross-module resolution supporting local file imports, named imports, and exported declarations across multiple files.
- **Deterministic Memory**: Implementation of the custom anchor/edge ownership graph model.

---

## Memory Philosophy

Galfus Script does not rely on a traditional global garbage collector or manual raw memory management. Instead, it utilizes an ownership model built on:

- **Anchors**: Roots that preserve value lifetime.
- **Edges**: Hard references connecting reachable values.
- **Weak Observers**: Non-owning references that are safely invalidated when the target value is released.

Values live as long as they are reachable from anchors through edges. When anchors or edges are removed, the affected graph fragments are released deterministically and cycle-safely at runtime.

Each execution thread owns a private heap. Inter-thread mailboxes transport byte sequences only; structured values require explicit serialization before they can be sent.

---

## Repository Layout

Galfus Script is structured as a cargo workspace containing the following crates:

```txt
galfus-script/
  ├── engine/
       ├── galfus-core/       # Shared IDs, diagnostics, spans, and primitive metadata
       ├── galfus-frontend/   # Lexer, parser, resolver, checker, and semantic validation
       ├── galfus-compiler/   # IR generation and bytecode compilation logic
       ├── galfus-ir/         # MIR representation and structures
       ├── galfus-bytecode/   # Bytecode format, validation, and in-memory executable graph
       ├── galfus-contract/   # Host-provider contracts, adapter schemas, and builtin source templates
       ├── galfus-workspace/  # Pipeline integration, incremental compilation, and embedded API
       ├── galfus-runtime/    # Runtime execution state, thread mailboxes, and Virtual Kernel
       └── galfus-vm/         # Virtual Machine interpreter and ownership graph engine
  ├── hosts/
       ├── galfus-cli/        # CLI interface (Command Line Interface) and native host provider
       └── galfus-playground/ # Web WASM playground and browser host provider
  └── examples/
       └── project/           # Sample workspace project with local main.gfs and config
```

---

## Virtual Standard Library

A minimal virtual standard library is available to user scripts, including:

### `std/io`

Offers basic console input/output interface:

- `fn read(terminator: [u8] = "\n"): [u8]`: Reads bytes until the delimiter or end of input. The delimiter is not returned.
- `fn print(text: [u8]): null`: Output a slice of u8 characters directly to the standard output.
- `fn println(text: [u8]): null`: Output a slice of u8 characters followed by a newline.

The workspace is host-neutral. A package that does not reference `std/io` can
run without an I/O provider. Once compilation records `std/io` as a provider
requirement, the execution host must supply a compatible provider during
preflight, before the first instruction runs. The CLI supplies native streams
and the playground supplies buffered streams. A host creates a sandbox by
omitting capabilities from the package or by rejecting the package during
preflight; operational I/O failures are delivered later through the normal
future/await result.

---

## Running and Testing Locally

### 1. Requirements

Ensure you have the latest Rust toolchain installed:

```bash
rustup update
```

### 2. Building the Project

Compile the workspace and CLI runner:

```bash
cargo build
```

### 3. Running the Example Project

Execute the sample workspace containing structures, array spreads, and control flow:

```bash
cargo run -- run examples/project
```

Expected output:

```txt
Hello Galfus!
Idade maior que 20
Program exited successfully with value: Null
```

### 4. Code & Semantic Auditing

Check the syntax and type checks of a single file:

```bash
cargo run -- check examples/project/src/main.gfs
```

Validate type-safety and semantics across the entire workspace directory:

```bash
cargo run -- check-workspace examples/project
```

### 5. Inspecting AST and Symbol Graph

Visualize AST nodes, scopes, references, and symbol tables:

```bash
cargo run -- graph examples/project/src/main.gfs
```

### 6. Executing Tests & Quality Checks

Run all unit and integration tests across the workspace:

```bash
cargo test --workspace
```

Ensure clippy checks and formattings are strictly clean:

```bash
cargo clippy --workspace --all-targets
cargo fmt --check
```

### 7. Testing the WebAssembly LSP (Playground)

To test the Galfus Language Server natively in the browser via WebAssembly, you can spin up the CodeMirror dev server:

```bash
bun cmd dev codemirror
```

This will automatically compile the `galfus-playground-web` WASM bindings and host a local web editor on `http://localhost:3000` connected to the engine via LSP JSON-RPC.

---

## Design Goals

Galfus Script is designed from the ground up to be:

- **VM-First**: Bytecode and interpreter structures dictate the design, making the VM highly portable.
- **Host integration**: Rust hosts can supply providers, drivers, and lifecycle
  control through explicit contracts; application scheduling and security policy
  remain host responsibilities.
- **Deterministic**: Standardized memory behavior, integer arithmetic, and strict execution paths.
- **Explicit**: Avoids magic conventions; imports, exports, and structures must be declared explicitly.

---

## Name Inspiration

The name **Galfus** comes from **Galafus**, a figure from Pernambuco folklore associated with will-o'-the-wisp phenomena. The wandering flame represents a runtime that is:

- Small & Portable
- Present where needed
- Lightweight by default
- Able to float across hosts and environments

---

## License

MIT
