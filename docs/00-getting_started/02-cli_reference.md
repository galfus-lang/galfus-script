# Galfus CLI Reference

The `galfus` executable (provided by `galfus-cli`) is the official command-line interface for the Galfus Script toolchain. It manages project initialization, validation, compilation, and execution.

## Installation

If you haven't installed the CLI yet, ensure you have Rust installed and build the project from the repository:

```bash
cargo build --release
# The binary will be available at target/release/galfus-cli
```

## Commands Overview

The CLI provides the following subcommands:

### `init`

Initializes a new Galfus project in the current directory.

```bash
galfus-cli init
```

This creates a default project structure (e.g., `src/main.gfs` and `galfus.toml`) ready for development.

### `run`

Executes the Galfus project. The CLI acts as the native host, providing
`std/io`, `std/env`, `std/time`, `std/fs`, `std/net`, `std/http`, and
`std/websocket` capabilities to the Virtual Machine.

```bash
galfus-cli run [WORKSPACE_DIR] [ARGS]...
```

- **`[WORKSPACE_DIR]`**: Path to the workspace directory. Defaults to the current directory (`.`).
- **`[ARGS]...`**: Additional arguments passed to the script's `main` function as `[[u8]]`.

*Example:*
```bash
galfus-cli run . --port 8080
```

### `check`

Validates the project for syntax, typing, and ownership errors without actually compiling the bytecode or running it.

```bash
galfus-cli check <WORKSPACE_DIR>
```

This is ideal for continuous integration (CI) environments or rapid feedback during development.

### `compile`

Compiles the project into an executable or binary bytecode format.

```bash
galfus-cli compile [OPTIONS] <WORKSPACE_DIR>
```

**Options:**
- `-t, --target <TARGET>`: Target architecture or platform (e.g., `x86_64-linux`).
- `-o, --out <OUT>`: Output path for the compiled artifact.
- `-p, --profile <PROFILE>`: Optimization profile. Default is `fastest`.

### `lsp`

Starts the Galfus Language Server Protocol (LSP) loop over standard input/output.

```bash
galfus-cli lsp
```

This command is used internally by code editors (such as VS Code) to provide features like autocomplete, real-time diagnostics, and hover tooltips.

### `upgrade`

Upgrades the Galfus CLI binary to a newer version.

```bash
galfus-cli upgrade
```

## Global Options

- `-h, --help`: Prints help information for any command.
- `-V, --version`: Prints the current version of the toolchain.
