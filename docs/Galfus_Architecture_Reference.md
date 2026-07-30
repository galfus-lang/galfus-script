# Galfus Architecture Reference

## 1. Core Identity

Galfus Script is a typed, VM-first scripting language with a straightforward pipeline. The primary pipeline is:

```text
.gfs Source
    ↓
Frontend (SemanticGraph)
    ↓
Compiler (BytecodeModule values)
    ↓
Workspace (BytecodeGraph)
    ↓
Runtime (Execution State)
    ↓
VM (Instruction Execution)
```

**What is implemented:**

- Full compiler pipeline (parsing to bytecode)
- In-memory `BytecodeGraph` execution
- Deterministic VM and memory graph
- Optional Host Providers boundary

**What is NOT part of the current architecture:**

- **GFB (Galfus Bytecode File):** Removed. The graph exists only in memory.
- **Bundler:** Not implemented.
- **Optimizer:** Not implemented.
- **Debugger:** Not implemented.
- **JIT Compilation:** Not implemented.

---

## 2. The Semantic Graph

The `SemanticGraph` represents the source-level meaning of the workspace.
It contains modules, symbols, typed references, and diagnostics.
The frontend processes source text and updates this graph.

---

## 3. The Bytecode Module

`BytecodeModule` is the isolated executable unit.
Each module contains its private and exported functions, globals, constants, layouts, and bytecode.
There is no global shared namespace. A variable without `export` belongs strictly to its module.

---

## 4. The Bytecode Graph

`BytecodeGraph` represents the complete executable program.
It contains multiple `BytecodeModule`s and their dependencies.
It is the only executable graph. The runtime does not rebuild or duplicate this graph.

The compiler produces a updated modules for changed modules. The
workspace applies it only to the declared graph version, validates the complete
result, and then publishes the next snapshot. Failed or stale transactions
leave the prior snapshot unchanged.

---

## 5. The Workspace

The `Workspace` owns the current architectural snapshots:

- Source state
- `SemanticGraph` snapshot
- `BytecodeGraph` snapshot

It manages the orchestration of the frontend, compiler, and provides an API for embedding.

---

## 6. Runtime and VM

The runtime executes an `Arc<BytecodeGraph>` with optional `Providers` and
optional `Adapters`.
Execution state lives in the VM and is partitioned by `ModuleId`, including
globals and initialization status. Dependencies initialize before the entry
module, and the runtime does not duplicate bytecode.

The `VM` executes bytecode instructions. It receives frames containing `ModuleId`, `FunctionId`, and `InstructionOffset`. Execution is implemented fundamentally via a `step` function that runs one instruction at a time.

Virtual threads provide cooperative concurrent execution. Each thread has an
isolated heap and mailbox; the runtime registry retains its identity, lifecycle
state, key, and mailbox while it is created, running, blocked, or exited.

### Cooperative Scheduler

The scheduler is implemented as a **FIFO queue** (`CooperativeDriver` backed by
a `VecDeque`). Threads are dispatched in arrival order — no thread can skip
ahead of another. Each scheduling cycle dequeues one thread, runs it for a
fixed instruction budget, and either re-enqueues it (still runnable) or
transitions it to a suspended state.

**Thread states:**

| State    | Description                                                            |
| :------- | :--------------------------------------------------------------------- |
| Runnable | In the FIFO queue, will be picked up in the next scheduling cycle.     |
| Running  | Currently executing its instruction budget on the driver.              |
| Blocked  | Suspended in `BlockedQueue`, waiting for an external event or timeout. |
| Exited   | Function returned; exit code is stored in the registry.                |

**I/O suspension:** When a thread emits a `VmEffect` (e.g. `ProviderCall` for
`println`), it is suspended and its continuation is stored. The I/O task is
dispatched to the **back** of the FIFO queue so the scheduler remains fair.
When the effect completes, the thread is resumed at the front of the queue to
continue promptly.

### Waiter Mechanism (`WaitThread`)

`WaitThread` implements join semantics without busy-waiting:

1. The calling thread emits `VmEffect::WaitThread { thread_id }`.
2. The orchestrator checks if the target is already `Exited`.
   - **Yes**: Resumes the caller immediately with the stored exit code.
   - **No**: Calls `kernel.block(caller)` and registers the caller in a
     `waiters: HashMap<ThreadId, Vec<WaiterEntry>>` on the `VirtualKernel`.
3. When the target thread exits (the `Exited` runtime event fires),
   `kernel.drain_waiters(target_id)` is called. Each waiter's thread is taken
   from the registry and resumed via `resume_or_fail_front`, putting it at the
   front of the FIFO queue with the exit code as its result.

---

## 7. Providers

Providers represent the boundary between Galfus and the host platform.
The provider surface is asynchronous and message-based. Concrete capabilities
are supplied by the embedding host.
If a provider is not supplied, related builtin calls will fail deterministically, allowing trivial sandboxing.

---

## 8. Execution Metadata

Each `BytecodeNode` may contain optional `ExecutionMetadata` with instruction
spans. Execution failures retain VM frames with module ID, function index, and
instruction offset across asynchronous suspension. The current structured
failure API does not expose resolved source spans directly.
Function-symbol and source-path mappings are planned metadata extensions.
