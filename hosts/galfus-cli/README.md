# galfus-cli

`galfus-cli` implements the command-line interface and entry point for the `galfus` executable.

## Responsibilities

- **Command-line Interface**: Parsers subcommands and arguments (such as `check`, `build`, and `run`) using CLI libraries.
- **Diagnostic Formatting**: Outputs clean, pretty-printed compiler diagnostics and errors to the terminal.
- **Runner Coordinator**: Invokes the appropriate pipeline stages in the runner crate.
