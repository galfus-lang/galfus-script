# Adapter System

While the _Provider System_ handles robust native capabilities, the **Adapter System** is specifically designed for Galfus' flexible **Foreign Function Interface (FFI)**.

## Proxy Modules (`.gfp`)

Whenever the execution of a Galfus script needs to trigger arbitrary logic from an external application (a custom database, a game engine it's embedded in, or an AI API), it won't use loose `.gfs` code, but rather the declaration of a _Proxy_.

Proxy modules have the `.gfp` extension and act as "abstract interfaces" that inform the Galfus compiler which function signatures exist and what parameters they need.

## How It Works

1. **Compilation and Validation**: The Workspace triggers a compatible `AdapterSchema` to ensure that the `.gfp` file is well-formed and semantically correct without running any _Host_ logic.
2. **Loading (Preflight)**: Only during execution preparation (Execution Host Preflight), the registered `AdapterModuleLoader` steps in, linking an `AdapterModuleBinding` to the `.gfp` declaration.
3. **Handle Management**: The adapter injects native resources, returning metadata and isolated calls cleanly, without polluting the language's kernel.

## Advantages

- This clear separation means you can compile your entire script, verify that everything is perfectly typed, bundle it, and only worry about FFI binaries at local orchestration time, bringing extreme portability between native and web environments.
