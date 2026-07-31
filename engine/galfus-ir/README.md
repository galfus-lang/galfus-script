# galfus-ir

`galfus-ir` defines the Medium-level Intermediate Representation (MIR) and lowering-facing structures.

## Responsibilities

- **MIR Definition**: Data structures for Basic Blocks, Operands, RValues, Statements, and Functions.
- **MIR Builder**: Lowering from resolved AST representations to clean MIR.
- **Lowering**: Code generation that lowers MIR into bytecode instructions suitable for the Galfus bytecode format.
