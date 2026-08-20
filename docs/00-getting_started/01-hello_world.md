# Conceptual Hello World

This guide presents how Galfus sees the execution of your code conceptually through the **Workspace**.
For practical day-to-day usage and command-line execution, refer to the [CLI Reference](./02-cli_reference.md).

## The `.gfs` Script

Galfus Script operates on files with the `.gfs` extension. They contain the declarations and the executable logic of the program.

```galfus
// hello.gfs
import { println } from "std/io"

export fn main(_args: [[u8]]): i32 {
    // The program's entry point function.
    println("Hello, Galfus World!")
    return 0
}
```

## The Workspace Concept

Unlike standalone scripts executed line by line, Galfus operates through a **Workspace**. The Workspace is a host-agnostic facade (independent of the operating system) responsible for:

1. Loading configurations (typically `WorkspaceConfig` via a TOML file).
2. Grouping modules and managing a source tree (`SourceStore`).
3. Strictly checking syntax and semantics (typing, ownership, contract validation).
4. Producing a coherent compilation report (`CompileReport`).

Your script code never runs directly. The Workspace builds and compiles the code into a rigorously validated _Bytecode_ graph. Only if no business or memory rules are violated is the program released for execution on the Virtual Machine.

This means that, despite being a scripting language, Galfus brings the robustness of a compiled language into the embedding process. In the next step of your learning journey, you will discover how the ecosystem handles host capabilities through _Providers_ and _Adapters_.
