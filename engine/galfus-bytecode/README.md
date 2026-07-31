# galfus-bytecode

`galfus-bytecode` defines the static bytecode representation, instruction format, and layout tables for the in-memory executable graph.

## Responsibilities

- **BytecodeModule**: Represents the isolated compiled artifact, containing constant pools, functions, types, struct/choice layouts, imports, and exports.
- **BytecodeGraph**: Represents the complete executable program graph containing multiple modules.
- **Instruction Format**: Specifies bytecode instruction opcodes, registers, and operand representations.
- **Bytecode Validation**: Validates instruction register bounds, jump target offsets, and type layout sanity.
- **In-Memory Graph**: The compiled `BytecodeGraph` lives entirely in memory and is directly consumed by the runtime.
