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

- [Embedding Galfus](docs/Embedding_Galfus_in_Rust.md)
- [Why Galfus Is Embeddable](docs/Why_Galfus_Is_Embeddable.md)
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
- **Encapsulated Builtins**: Strictly prevents user projects from referencing or declaring `__builtin_*` compiler intrinsics directly. These are visible only inside compiler-trusted builtin scopes.
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
  ├── crates/
       ├── galfus-core/       # Shared IDs, diagnostics, spans, and primitive metadata
       ├── galfus-frontend/   # Lexer, parser, resolver, checker, and semantic validation
       ├── galfus-ir/         # MIR representation and VM lowering code
       ├── galfus-bytecode/   # Bytecode format, validation, and in-memory executable graph
       ├── galfus-contract/       # Optional host-provider contracts used at execution time
       ├── galfus-runtime/    # Runtime execution state, module globals, and VM setup
       ├── galfus-vm/         # Virtual Machine interpreter and ownership graph engine
       ├── galfus-builtins/   # Standard library builtins and rich_builtins files
       ├── galfus-runner/     # Workspace compilation pipeline and executor
       └── galfus-cli/        # CLI interface (Command Line Interface)
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

The workspace is host-neutral. A host optionally supplies `Providers` when
calling `Workspace::run`; the CLI supplies native streams and the playground
supplies buffered streams. A program that reaches `std/io` without an I/O
provider fails at runtime, so a host can create a sandbox by supplying none.

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
