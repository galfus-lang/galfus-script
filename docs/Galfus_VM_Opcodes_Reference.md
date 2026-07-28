# Galfus VM Bytecode and Opcode Specification

> **Status: Historical design.** The current VM executes the in-memory Rust
> `Instruction` representation. It has no serialized bytecode format,
> variable-length binary encoding, or multithreading.

This document preserves an earlier binary instruction-set proposal for the
Galfus Virtual Machine.

---

## 1. Bytecode Instruction Format

Galfus VM uses a **register-based execution model** where instructions read operands from and write results to local slots (registers) within the current call frame.

To keep the serialized instruction stream compact, instructions use a **variable-length encoding**:

- **Opcode**: 1 byte (`u8`).
- **Registers**: 2 bytes (`u16`), supporting up to 65,536 registers per call frame.
- **Immediate / Indices**: 2 bytes (`u16`) or 4 bytes (`u32`) for indexing constant pools, types, functions, and globals.

### Operand Types in Bytecode

- **`Reg`**: A 16-bit register index (`r0`, `r1`, ...).
- **`ConstIdx`**: A 16-bit index into the function's or module's Constant Pool.
- **`TypeIdx`**: A 16-bit index into the global/module Type Table.
- **`FuncIdx`**: A 16-bit index into the module's Function Table.
- **`GlobalIdx`**: A 16-bit index into the module's Globals Table.
- **`FieldIdx`**: A 16-bit field offset index for structs.
- **`Offset`**: A signed 32-bit relative bytecode instruction offset (`i32`) for jumps.

---

## 2. Instruction Set

### Category A: Data Movement & Constants

| Opcode             | Hex    | Arguments                            | Behavior                                  |
| :----------------- | :----- | :----------------------------------- | :---------------------------------------- |
| **`LOAD_CONST`**   | `0x01` | `dest: Reg`, `const_idx: ConstIdx`   | Loads constant from the pool into `dest`. |
| **`MOVE`**         | `0x02` | `dest: Reg`, `src: Reg`              | Copies value from `src` to `dest`.        |
| **`LOAD_GLOBAL`**  | `0x03` | `dest: Reg`, `global_idx: GlobalIdx` | Loads value from global slot to `dest`.   |
| **`STORE_GLOBAL`** | `0x04` | `global_idx: GlobalIdx`, `src: Reg`  | Stores value from `src` into global slot. |
| **`LOAD_NULL`**    | `0x05` | `dest: Reg`                          | Loads `null` value into `dest`.           |

---

### Category B: Unary & Binary Operations

All mathematical and comparison operations are type-safe. The VM validates the runtime tags of operands to ensure correctness.

| Opcode         | Hex    | Arguments                                | Behavior                              |
| :------------- | :----- | :--------------------------------------- | :------------------------------------ |
| **`ADD`**      | `0x10` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = lhs + rhs`                    |
| **`SUB`**      | `0x11` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = lhs - rhs`                    |
| **`MUL`**      | `0x12` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = lhs * rhs`                    |
| **`DIV`**      | `0x13` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = lhs / rhs`                    |
| **`REM`**      | `0x14` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = lhs % rhs`                    |
| **`POW`**      | `0x15` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = lhs ^ rhs`                    |
| **`NEG`**      | `0x16` | `dest: Reg`, `src: Reg`                  | `dest = -src`                         |
| **`NOT`**      | `0x17` | `dest: Reg`, `src: Reg`                  | `dest = !src`                         |
| **`BIT_NOT`**  | `0x18` | `dest: Reg`, `src: Reg`                  | `dest = ~src`                         |
| **`SHL`**      | `0x19` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = lhs << rhs`                   |
| **`SHR`**      | `0x1A` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = lhs >> rhs`                   |
| **`AND`**      | `0x1B` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = lhs & rhs`                    |
| **`OR`**       | `0x1C` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = lhs \| rhs`                   |
| **`XOR`**      | `0x1D` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = lhs ^ rhs`                    |
| **`EQ`**       | `0x20` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = (lhs == rhs)`                 |
| **`NE`**       | `0x21` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = (lhs != rhs)`                 |
| **`LT`**       | `0x22` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = (lhs < rhs)`                  |
| **`LE`**       | `0x23` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = (lhs <= rhs)`                 |
| **`GT`**       | `0x24` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = (lhs > rhs)`                  |
| **`GE`**       | `0x25` | `dest: Reg`, `lhs: Reg`, `rhs: Reg`      | `dest = (lhs >= rhs)`                 |
| **`FALLBACK`** | `0x26` | `dest: Reg`, `src: Reg`, `fallback: Reg` | `dest = src != null ? src : fallback` |

---

### Category C: Control Flow & Subroutines

Control flow uses relative bytecode jumps. Jumps are offset by instructions/bytes relative to the program counter (`PC`).

| Opcode           | Hex    | Arguments                                                        | Behavior                                            |
| :--------------- | :----- | :--------------------------------------------------------------- | :-------------------------------------------------- |
| **`JUMP`**       | `0x30` | `offset: i32`                                                    | Unconditionally jump to `PC + offset`.              |
| **`JUMP_TRUE`**  | `0x31` | `cond: Reg`, `offset: i32`                                       | Jump to `PC + offset` if `cond` is truthy.          |
| **`JUMP_FALSE`** | `0x32` | `cond: Reg`, `offset: i32`                                       | Jump to `PC + offset` if `cond` is falsy.           |
| **`JUMP_NULL`**  | `0x33` | `val: Reg`, `offset: i32`                                        | Jump to `PC + offset` if `val` is `null`.           |
| **`CALL`**       | `0x34` | `dest: Reg`, `func: FuncIdx`, `args_start: Reg`, `arg_count: u8` | Invoke function with arguments in contiguous slots. |
| **`RET`**        | `0x35` | `src: Reg`                                                       | Return value from `src` to caller.                  |
| **`RET_NULL`**   | `0x36` | None                                                             | Return `null` to caller (implicit void return).     |
| **`PANIC`**      | `0x37` | `const_idx: ConstIdx`                                            | Terminate process with error constant message.      |

---

### Category D: Heaps, Structs & Collections

Handles allocation in the current thread's private heap, object instantiations, and property/index reads/writes.

| Opcode             | Hex    | Arguments                                                            | Behavior                                                       |
| :----------------- | :----- | :------------------------------------------------------------------- | :------------------------------------------------------------- |
| **`ALLOC_LOCAL`**  | `0x40` | `dest: Reg`, `type_idx: TypeIdx`                                     | Allocate a struct on the **Local Heap**.                       |
| **`LOAD_FIELD`**   | `0x42` | `dest: Reg`, `obj: Reg`, `field: FieldIdx`                           | Read field from a local struct/object.                         |
| **`STORE_FIELD`**  | `0x43` | `obj: Reg`, `field: FieldIdx`, `val: Reg`                            | Write value to local struct/object field.                      |
| **`NEW_ARRAY`**    | `0x44` | `dest: Reg`, `type_idx: TypeIdx`, `len_reg: Reg`                     | Create array of length `len_reg`.                              |
| **`LOAD_INDEX`**   | `0x45` | `dest: Reg`, `arr: Reg`, `idx: Reg`                                  | Read element `arr[idx]`. Returns `null` if out of bounds.      |
| **`STORE_INDEX`**  | `0x46` | `arr: Reg`, `idx: Reg`, `val: Reg`                                   | Write `arr[idx] = val`. Panics/throws error if out of bounds.  |
| **`NEW_TUPLE`**    | `0x47` | `dest: Reg`, `type_idx: TypeIdx`, `start: Reg`, `count: u8`          | Create tuple from contiguous registers.                        |
| **`NEW_CHOICE`**   | `0x48` | `dest: Reg`, `type_idx: TypeIdx`, `variant_idx: u16`, `payload: Reg` | Create choice (tagged union) variant. payload can be `null`.   |
| **`CAST`**         | `0x49` | `dest: Reg`, `src: Reg`, `type_idx: TypeIdx`                         | Safe type cast. Panics if types are incompatible.              |
| **`INSTANCEOF`**   | `0x4A` | `dest: Reg`, `src: Reg`, `type_idx: TypeIdx`                         | Sets `dest` to `true` if `src` matches type, else `false`.     |

---

### Category E: Memory Ownership (Owner Graph Integration)

The VM uses these opcodes to coordinate with the `owner_graph_core` for deterministic release.

| Opcode     | Hex    | Arguments  | Behavior                                                                                            |
| :--------- | :----- | :--------- | :-------------------------------------------------------------------------------------------------- |
| **`DROP`** | `0x50` | `reg: Reg` | Signals to `owner_graph_core` to release the anchor/root mapped to `reg`. The slot is marked empty. |

---

## 3. Bytecode Execution Semantics

### Call Frames and Register File

Each function call allocates a `CallFrame`:

- **Instructions Pointer (PC)**: Points to the next bytecode instruction to execute.
- **Register File**: An array of VM values representing the function's parameters and local variables.
  - Register `r0` is typically the return value placeholder or the first parameter.
  - Local registers are cleared (`null` or uninitialized) when a frame is created.
  - When executing `DROP reg`, the value in `reg` is released from the frame's roots.

### Unwinding and Rollback Loop

The interpreter's loop operates with exception/panic safety:

1. Fetch opcode at `PC`.
2. Decode operands.
3. Dispatch execution.
4. If a panic or termination is triggered:
   - Walk up the call stack.
      - Continue unwinding until caught or the VM aborts.
