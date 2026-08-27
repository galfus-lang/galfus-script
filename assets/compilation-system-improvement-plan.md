# Compilation System Improvement Plan

## Objective

Make `Workspace::compile()` the sole public compilation entry point and ensure
that every successful compilation returns a validated, optimized package.

The work combines the current bytecode optimizer with the identified
recursion, register, CFG, literal, call, and MIR-pass opportunities. It must
preserve language semantics, ownership, async/Future behavior, providers,
adapters, dynamic calls, module imports, bytecode compatibility, and existing
resource limits.

## Target architecture

```text
source
  -> frontend check
  -> semantic-to-MIR
  -> verified MIR optimization pipeline
  -> SSA lowering
  -> bytecode emission
  -> package reachability pruning
  -> bytecode canonicalization and compaction
  -> bytecode/package validation
  -> Workspace::compile() result
```

`Workspace::optimize()` will be removed. Neither the CLI nor a host will be
responsible for deciding whether an executable package is optimized.

`compile()` remains incremental: it recompiles only the affected semantic
modules, then runs the finalization needed to make the complete package
consistent. Early phases may finalize the full graph for correctness. Later
phases must avoid cloning or rewriting unchanged modules.

## Invariants

- `check()` remains the gate before compilation.
- A package returned by `compile()` is executable and optimized.
- Recompiling an unchanged semantic revision returns the cached final package.
- Every bytecode transform preserves instruction spans or remaps them with the
  instruction offset, so stack traces remain meaningful.
- Relative jump targets, imports, function IDs, constants, type indices,
  layouts, and register windows remain valid after every transform.
- Dynamic calls, method calls, provider exports, adapter proxies, and public
  exports keep their conservative reachability semantics.
- No optimization changes the algorithm used by the benchmark. In particular,
  the tree-recursive Fibonacci benchmark remains recursive.

## Phase 0 — Baseline, contracts, and test coverage

### Scope

- Record the current optimized bytecode baselines for `fib`, `matrix4`, and
  `tasks`: instructions, registers, constants, calls, branches, and package
  size.
- Add focused compiler/bytecode test fixtures for branch joins, loops,
  recursion, tail calls, local/imported/dynamic calls, async/Future operations,
  providers, adapters, and ownership-bearing values.
- Define a bytecode-transform contract: input validation, transformation,
  output validation, span remapping, and before/after statistics.
- Correct the opcode decoder coverage so every current instruction tag,
  including `CallInternalThread` (71), is accepted and tested.

### Acceptance criteria

- Baseline artifacts are deterministic.
- New bytecode validation tests cover all operand-bearing instruction forms.
- A malformed transform is rejected before it reaches the runtime.

## Phase 1 — Absorb optimization into `compile()`

### Scope

1. Split the current package construction portion of `compile()` into an
   internal finalization step.
2. Invoke bytecode finalization from `compile()` after graph construction and
   before storing `CompileState::Ready`.
3. Remove public `Workspace::optimize()` and its optimizer-only error path.
4. Simplify CLI `run` and `compile` to call only `workspace.compile()`.
5. Update workspace/host tests that currently call `compile()` followed by
   `optimize()`, or rely on unoptimized output.
6. Keep an internal test-only way to inspect pre-finalization bytecode when a
   test needs to assert an emitter property.

### Design decision

Do not make optimization optional in the public production API. If raw
bytecode is useful for compiler debugging, expose it only through an internal
test helper or a debug-only compiler option that cannot accidentally reach a
production host.

### Acceptance criteria

- CLI, native host, `PackageLoader`, and workspace execution receive identical
  final bytecode for the same revision.
- Repeated `compile()` without source changes returns the cached package.
- Existing package behavior and bytecode validation remain unchanged except
  for intentional optimizer output.

## Phase 2 — Prune package reachability before per-function optimization

### Scope

1. Retain and harden the current function/constant reachability analysis after
   bytecode emission.
2. Run it before CFG canonicalization, register analysis, and compaction, so
   unreachable functions and their constants do not consume optimization work.
3. Preserve conservative roots: module initialization, exports, adapter
   proxies, dynamic method candidates, and all required import targets.
4. Remap functions, imports, constants, exports, and debug metadata once after
   pruning.
5. Emit pruning statistics: functions/constants retained and removed per
   module.

### Design decision

The initial prune is mandatory because it reduces the input of every later
per-function pass. A second prune runs only after a later pass can remove or
rewrite calls; the early CFG and register passes do not need it because they
preserve the call graph.

### Acceptance criteria

- No unreachable function is canonicalized or register-compacted.
- Exported, dynamic-method, provider, and adapter functions remain reachable.
- Function/import/constant remapping and instruction spans remain valid.

## Phase 3 — Replace the bytecode cleanup loop with canonical CFG cleanup

### Scope

Replace the repeated scan/clone/rewrite loop in `workspace::optimizer` with a
single CFG-aware canonicalization pipeline per function:

1. Build successors and entry reachability from instruction zero.
2. Remove unreachable instructions without relying on a linear `dead` flag.
3. Remove `Move { dest == src }`.
4. Remove `Jump` instructions targeting their immediate successor.
5. Thread chains of unconditional jumps when doing so does not alter a
   conditional branch's semantics.
6. Rebuild instruction offsets and debug spans once after the final retained
   instruction sequence is known.
7. Return an unchanged marker when no instruction changed.

### Why first

This is low risk, immediately eliminates the redundant Fibonacci base-path
jump, and establishes a reusable CFG representation for later liveness and
register allocation work.

### Acceptance criteria

- Branch, loop, panic, return, and jump-target tests remain correct.
- Fibonacci loses its jump-to-next instruction.
- The pass performs one retained-instruction rewrite, not one per cleanup
  iteration.

## Phase 4 — Compact registers and frame layout

### Scope

1. Add a bytecode register-use visitor covering every instruction.
2. First implement a safe dense remap: parameters retain their ABI positions;
   used non-parameter locals are compacted; unused holes are removed; required
   contiguous operand windows remain contiguous.
3. Rewrite every register reference, then calculate `local_count` and
   `temp_count` from the compact layout.
4. Validate register bounds and contiguous ranges after rewriting.
5. Only after the dense remap is stable, add liveness-based reuse of
   non-overlapping local intervals.

### Expected impact

The current Fibonacci frame reserves 15 slots despite unused registers
`1..6`. `matrix4` reserves 425 slots for 314 instructions. Dense compaction
addresses both without changing their computation.

### Risks

`Call`, `CreateFuture`, `AwaitAll`, aggregate construction, and parallel-copy
lowering use contiguous register ranges. The allocator must model these as
fixed-width intervals, not independent register operands.

### Acceptance criteria

- Fibonacci and matrix register counts decrease from their baseline.
- Ownership, arrays, structs, tuples, choices, async, and call argument tests
  pass with compacted registers.
- No transform leaves an invalid register or range.

## Phase 5 — Emitter-level redundant instruction removal

### Scope

Move optimizations that require semantic/type knowledge to bytecode emission:

1. Do not emit a `Cast` after a typed literal when the constant representation
   is already the destination type.
2. Lay out simple conditional blocks to prefer fall-through and avoid an
   unconditional join jump where possible.
3. Emit direct moves only when source and destination differ.
4. Preserve the generic bytecode cleanup pass as a backstop for cases produced
   by other lowerings.

### Acceptance criteria

- Matrix's redundant literal casts decrease.
- The optimizer remains valid for bytecode from all emitters, including
  generated or deserialized packages.

## Phase 6 — Compact hot-path instruction forms

### Scope

Introduce new bytecode forms only after Phase 4 produces a stable register
model:

1. A direct one-argument local-call form accepting an arbitrary source
   register. This removes the temporary argument move at common recursive
   single-argument call sites.
2. Typed immediate forms for `i32`, `u32`, `i64`, `u64`, `f32`, and `f64`.
   Arithmetic and comparisons receive forms such as `LeI32Imm`, `SubI32Imm`,
   `LeU32Imm`, `SubU64Imm`, `LeF32Imm`, and `MulF64Imm`. Bitwise and shift
   immediate forms are limited to integer families, for example `AndU32Imm`
   and `ShlI64Imm`; floats do not support bitwise operations.
3. Matching VM fast paths, bytecode validation, opcode decoding, package
   format/version policy, and serialization tests.

### Type and representation policy

- Each immediate uses its exact operand type. Do not promote `i32` to `i64`,
  or `f32` to `f64`, to implement an instruction.
- Float immediates retain their IEEE bit pattern, including signed zero,
  infinities, and NaN behavior required by the language.
- Keep register/register instructions for dynamic operands. Select an
  immediate form only when the right-hand operand is a matching compile-time
  literal.
- Apply constant folding first when it removes the operation entirely;
  immediate forms optimize the remaining variable-plus-literal cases.

### Non-goals

- Do not specialize imported calls in the first implementation.
- Do not remove call-depth checks, ownership handling, or runtime quota
  behavior.
- Do not add a JIT or rewrite Fibonacci into an iterative algorithm.

### Acceptance criteria

- The two recursive Fibonacci sites lose their argument move and constant
  loads where the `i32` immediate form applies.
- Results, stack limits, error behavior, and stack traces are unchanged.
- Package decoding accepts every declared current opcode.
- Integer wrapping, unsigned behavior, and `f32`/`f64` rounding remain exactly
  the same as their register/register counterparts.

## Phase 7 — Introduce a verified MIR pass manager

### Scope

The existing `inline_functions`, `optimize_mir`, and `optimize_tail_calls`
implementations are not part of the compile path. Replace ad-hoc activation
with a pass manager and explicit invariants.

Proposed order:

```text
MIR construction
  -> safe local simplification / constant propagation
  -> size-bounded non-recursive inlining
  -> tail-call recognition
  -> SSA conversion
  -> SSA-aware copy propagation and dead-definition elimination
  -> bytecode emission
```

### Rules per pass

- **Copy propagation:** use SSA definitions and dominance, never a global
  name-to-name replacement across unrelated blocks.
- **Inlining:** only synchronous, non-recursive, non-external candidates below
  an instruction and local-count budget. Do not inline across a provider,
  adapter, async boundary, or dynamic call.
- **TCO:** only a proven tail call. It cannot improve tree-recursive
  Fibonacci because its calls are followed by addition.
- **Constant folding:** fold pure primitive operations only when overflow,
  division-by-zero, and language numeric semantics are preserved.
- **Dead definitions:** do not remove values with ownership drops, allocation,
  calls, future creation, or other observable effects.

### Acceptance criteria

- Each pass is independently enableable in tests and reports its delta.
- No pass increases code size beyond its configured budget unless it produces
  a documented reduction in calls.
- Recursive, async, ownership, provider, adapter, and dynamic-call test suites
  remain valid.

## Phase 8 — Incremental finalization and post-transform pruning

### Scope

1. Reuse borrowed method names or interned identifiers in package-level
   reachability instead of cloning strings unnecessarily.
2. Mark modules unchanged by local canonicalization/compaction and preserve
   their nodes rather than cloning every module.
3. Build an invalidation set for global method reachability and dynamic-call
   conservatism; only affected modules are re-pruned/remapped.
4. Run a second reachability prune only after an MIR or bytecode pass has
   removed/replaced calls; skip it when the call graph is unchanged.
5. Add an idempotency fast path: finalizing an already-final package returns
   the existing package allocation.
6. Consider type/layout pruning only after proving that all reflective,
   provider, adapter, and serialization requirements are represented in the
   reachability graph.

### Acceptance criteria

- Incremental compilation avoids rewriting unrelated module bytecode.
- Finalization is deterministic and idempotent.
- Dynamic method dispatch and adapter proxy reachability remain conservative.

## Phase 9 — Measurement and rollout

### Measurements

For each phase, collect release no-metrics medians for Fibonacci, Matrix, and
CPU Tasks; use metrics builds only to attribute runtime behavior. Record:

```text
compile wall time
package byte size
per-function instruction count
per-function local/temp/frame register count
constant/type/layout counts
branch/direct-call/import-call/dynamic-call counts
optimizer before/after deltas
Fibonacci VM dispatches, argument moves, local calls, and peak frame slots
```

### Rollout gates

1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. Bytecode encode/decode and validation suites
5. Provider, adapter, cancellation, mailbox, timeout, ownership, and
   concurrency suites
6. Multi-sample benchmarks compared with the recorded baseline and QuickJS

## Priority order

| Priority | Phase | Value |
| --- | --- | --- |
| P0 | 0–1 | One consistent compilation contract and safety net |
| P1 | 2 | Reduces the input to every per-function optimization pass |
| P1 | 3 | Faster, smaller canonical bytecode with low risk |
| P1 | 4 | Largest general frame/cache reduction |
| P2 | 5–6 | Removes Fibonacci hot-path instructions |
| P2 | 7 | Safely activates broader compiler optimizations |
| P3 | 8 | Improves incremental compilation and package size |
| P0 throughout | 9 | Prevents performance claims without evidence |
