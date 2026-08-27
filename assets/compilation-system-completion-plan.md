# Compilation System Completion Plan

## Purpose

Complete the remaining work from `compilation-system-improvement-plan.md`
without treating a partial implementation as a completed phase. This plan is a
successor plan: it records the state observed in the current worktree, defines
the missing deliverables, and makes each phase independently verifiable.

The scope is the compilation and finalization pipeline:

```text
checked source
  -> semantic MIR
  -> verified MIR passes
  -> bytecode emission
  -> package pruning
  -> CFG/register canonicalization
  -> package validation
  -> cached Workspace::compile() result
```

Runtime scheduling, Future behavior, providers, adapters, and benchmark
algorithms are out of scope except where a compiler change must preserve their
observable semantics.

## Confirmed current state

| Area | Status | Evidence / remaining gap |
| --- | --- | --- |
| `compile()` owns finalization | Complete | Workspace compilation finalizes and caches the package; public optimizer use was removed from the CLI path. |
| Initial package reachability prune | Partial | Functions/constants are pruned before function cleanup and unchanged package nodes can be retained. Per-module statistics, invalidation, and a conditional second prune are absent. |
| CFG cleanup | Partial | Reachability, no-op moves, jump-to-next removal, and jump threading exist. The implementation still performs a clone-and-repeat cleanup loop, contrary to the one-rewrite target. |
| Dense register compaction | Complete as a baseline | All instruction operands and contiguous ranges are visited and remapped. It removes holes only; it does not reuse registers based on liveness. |
| Emitter redundancy removal | Partial | Typed literal casts, self-moves, and selected direct call argument handling were addressed. The intended conditional fall-through layout needs measured compiler-output coverage. |
| Hot instruction forms | Partial | Arbitrary-source one-argument calls and typed `BinaryImmediate` execution/validation exist. Compiler-output tests must demonstrate selection for every supported numeric family and preserve float edge cases. |
| MIR pass manager | Partial | Validation, identity simplification, bounded inlining, and direct self-tail-call recognition are active. Constant propagation, SSA-aware copy propagation, and ownership-safe dead-definition elimination are absent. Pass ordering around SSA must be made explicit and verified. |
| Incremental finalization | Partial | Idempotency and unchanged-node preservation exist. Dependency-based invalidation and post-call-transform pruning do not. |
| Benchmark and rollout evidence | Partial | The harness samples cold processes and includes QuickJS. It does not persist a reproducible baseline/report or collect all compilation/bytecode deltas. |

`Complete` above means only that the stated baseline is present. It does not
mean that the original broad phase is fully accepted.

## Non-negotiable invariants

- `Workspace::compile()` is the only public production compilation path.
- Every package returned by `compile()` validates before execution.
- Unchanged semantic revisions return the cached final package.
- Bytecode transforms preserve or remap instruction offsets used by metadata.
- Parameters keep ABI positions. Operand windows remain contiguous after any
  register allocation pass.
- No optimization removes or changes an ownership drop, allocation, call,
  Future creation, provider/adapter action, panic, or other observable effect.
- Dynamic methods, exports, imports, module initialization, providers, and
  adapter proxies remain conservative roots for reachability.
- Benchmarks retain their algorithms; Fibonacci remains tree-recursive.

## Phase A — Establish a reproducible completion baseline (complete)

### Implementation status

- Structural package statistics and the `GALFUS_DEBUG_BYTECODE_STATS` CLI
  output are implemented.
- Release artifacts for Fib, Matrix4, and Tasks are recorded in
  `compilation-baseline.md` and the corresponding `.jsonlog` files.
- Optimizer transform-contract tests validate a malformed input is rejected
  and a loop/branch transform remains valid.
- The transform helper now also asserts retained instruction offsets keep a
  source location. Existing focused coverage is retained in bytecode
  validation (all operand forms and futures), package adapter-manifest tests,
  workspace provider execution tests, MIR async lowering tests, and MIR
  tail-call/external-call pass tests.

### Deliverables

1. Run bytecode validation and focused workspace/compiler tests before further
   optimization changes.
2. Record deterministic before/after artifacts for `fib`, `matrix4`, and
   `tasks`: package bytes; function/instruction/register/constant counts;
   call and branch counts.
3. Add or confirm fixtures covering branches, loops, recursive calls, local,
   import and dynamic calls, async/Future operations, providers, adapters, and
   ownership-bearing values.
4. Define one transform test helper that validates input, runs a transform,
   validates output, and checks instruction-offset metadata remapping.

### Exit criteria

- A future phase can compare its result to a named baseline artifact.
- The test helper rejects malformed bytecode before runtime execution.

## Phase B — Finish canonical CFG cleanup (complete)

### Implementation status

- Canonicalization now reaches a fixed point over a removal bitmap and rebuilds
  the function once; it no longer clones and reprocesses rewritten bytecode.
- Unconditional chains are threaded for both unconditional and conditional
  targets, while conditional fall-through remains intact.
- The remaining Fibonacci jump skips the base-case `Ret`; it is not a
  jump-to-next and must remain until Phase D can choose a better emitter
  layout.

### Deliverables

1. Replace `optimize_function`'s clone-and-repeat loop with explicit CFG
   analysis and a fixed-point jump-target resolution that does not repeatedly
   clone a function.
2. Compute reachability, removable instructions, final targets, and the
   old-to-new offset map before constructing the retained instruction vector.
3. Retain semantics for conditional successors, backward loop edges, returns,
   panics, and invalid-target rejection.
4. Update metadata exactly once from the final old-to-new mapping.

### Exit criteria

- A changed function is reconstructed once per canonicalization pass.
- No-op jumps, no-op moves, unreachable code, and unconditional jump chains
  are eliminated in one final result.
- Branch, loop, recursion, panic, and span-remapping tests pass.

## Phase C — Add CFG-aware liveness register reuse

### Deliverables

1. Build basic blocks and successor/predecessor relations from canonical
   bytecode.
2. Compute backward `live_in`/`live_out` sets to a fixed point, including
   values used by branch conditions and return/panic operands.
3. Treat every contiguous operand window as a fixed-width allocation request;
   calls, futures, aggregate operations, and parallel copies cannot be split.
4. Allocate only non-parameter registers. Preserve parameters and all ABI
   windows; reuse a register only when liveness proves that its prior value is
   dead on every path.
5. Keep dense compaction as a preliminary normalization and provide a
   conservative fallback when a function shape is unsupported.

### Exit criteria

- Register bounds and contiguous ranges validate after allocation.
- Tests cover branch joins, loops, calls with argument ranges, `AwaitAll`,
  aggregates, ownership values, and async operations.
- Frame counts improve or remain equal; never increase merely because this
  pass ran.

## Phase D — Close emitter and hot-form gaps

### Deliverables

1. Confirm with compiler-output tests that redundant typed-literal casts and
   self-moves are not emitted.
2. Implement conditional fall-through layout only for proven simple shapes;
   preserve a generic fallback for all other CFGs.
3. Test direct one-local-argument calls using non-zero source registers.
4. Test immediate lowering and VM behavior for `i32`, `u32`, `i64`, `u64`,
   `f32`, and `f64`; test integer wrapping, unsigned comparisons, signed zero,
   infinities, and NaNs. Integer bitwise/shift immediate forms stay limited to
   integer families.
5. Keep the current generic typed `BinaryImmediate` representation unless
   profiling proves opcode-specific forms outperform its dispatch and justify
   bytecode-format expansion.

### Exit criteria

- Fibonacci compiler output proves the two recursive calls and literal
  operations use the compact forms where type checking permits them.
- Decoder, encoder, validator, and VM tests cover every currently emitted
  opcode/form.

## Phase E — Complete the verified MIR optimization pipeline

### Deliverables

1. Establish and test the actual SSA boundary. MIR construction, pre-SSA
   passes, SSA conversion, and post-SSA passes must execute in the documented
   order exactly once.
2. Add safe local constant propagation/folding for pure primitive operations
   only, with exact overflow, division-by-zero, integer, and floating-point
   semantics.
3. Add SSA-aware copy propagation using definitions and dominance rather than
   global name replacement.
4. Add ownership-safe dead-definition elimination. Values with drops,
   allocations, calls, futures, panics, or externally visible effects remain.
5. Expand pass reports to include each pass delta and enforce a configured
   code-size budget for inlining.
6. Keep inlining synchronous, non-recursive, non-external, and bounded;
   retain only proven direct self-tail-call rewriting.

### Exit criteria

- Each pass can be enabled independently in tests.
- Every pass validates MIR before and after execution.
- Recursive, async, ownership, provider, adapter, and dynamic-call cases
  prove that no unsafe transformation occurs.

## Phase F — Finish incremental package finalization

### Deliverables

1. Track dependency/invalidation inputs for global method names and dynamic
   dispatch candidates so only affected modules are re-pruned.
2. Preserve bytecode graph nodes for modules whose final bytes and metadata do
   not change.
3. Let MIR/bytecode passes report whether they changed the call graph.
4. Run a second package reachability prune only when that report is true;
   otherwise skip it.
5. Emit per-module retained/removed function and constant counts through a
   debug/telemetry path that has no release cost when disabled.
6. Consider type/layout pruning only after an explicit reachability model
   covers reflection, provider, adapter, serialization, and public ABI needs.

### Exit criteria

- Recompiling an unrelated module does not rewrite unrelated final nodes.
- Finalization of an already-final package returns the original allocation.
- Dynamic dispatch and adapter/proxy reachability tests remain conservative.

## Phase G — Measurement, regression gates, and release decision

### Deliverables

1. Make the benchmark harness reproducible: permit reuse of existing release
   binaries, record tool versions and commands, and persist the raw samples
   plus median summary under `.tmp/`.
2. Record release/no-metrics medians for Fibonacci, Matrix, and CPU Tasks;
   use metrics builds only to explain costs.
3. Capture compile wall time, package bytes, bytecode counts, optimizer/MIR
   deltas, Fib dispatches/argument moves/peak frame slots, and relevant
   runtime event counters.
4. Compare against the Phase A baseline and QuickJS without changing a
   benchmark algorithm.
5. Run format, Clippy with warnings denied, workspace tests, bytecode
   encode/decode/validation, and provider/adapter/cancellation/mailbox/timeout
   concurrency suites.

### Exit criteria

- Raw benchmark data and environment are available for reproduction.
- Every claimed optimization has a correctness result and a measured delta.
- Release decision names regressions explicitly; lack of a speedup is not
  treated as completion failure if correctness and size goals are met.

## Execution order and stop conditions

| Order | Phase | Prerequisite | Stop condition |
| --- | --- | --- | --- |
| 1 | A | None | Baseline and transform contract are reproducible. |
| 2 | B | A | CFG performs one reconstruction and passes control-flow tests. |
| 3 | C | B | Liveness allocation validates every supported operand range. |
| 4 | D | B | Emission and hot-form selection are demonstrated by output tests. |
| 5 | E | A | MIR order and every enabled pass are validated. |
| 6 | F | B and E call-graph reports | Incremental and second-prune decisions are explicit. |
| 7 | G | A–F | Results, gates, and any remaining regressions are recorded. |

Do not start a dependent phase when its prerequisite exit criteria are not
met. If a required optimization is not profitable, retain the safe baseline,
record the measurement, and close that item as deliberately declined rather
than silently calling it complete.
