# galfus-frontend

`galfus-frontend` implements the parser, name resolver, and type checker for the Galfus language.

## Responsibilities

- **Lexing & Parsing**: Tokenizes and parses raw Galfus source text into a green tree / AST.
- **Name Resolution**: Resolves symbol paths, handles imports/exports, and builds the local and workspace symbol graphs.
- **Type Checking**: Infers and validates expressions, structs, choices, and constraint applications.
- **Semantic Validation**: Performs AST-level safety and correctness checks.
