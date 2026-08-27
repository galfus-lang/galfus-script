# Bytecode Recursion and Optimizer Analysis

Date: 2026-08-26

## Scope and method

This is an analysis-only report. No runtime, compiler, or benchmark source was
changed.

The package path was inspected from semantic lowering through execution, and
the release CLI emitted the optimized bytecode for the current benchmarks:

```text
GALFUS_DEBUG_BYTECODE=1 ./target/release/galfus-cli run benchmark/fib.gfs
GALFUS_DEBUG_BYTECODE=1 ./target/release/galfus-cli run benchmark/matrix4.gfs
```

The raw debug outputs are available temporarily as:

- `.tmp/fib-bytecode-analysis.log`
- `.tmp/matrix4-bytecode-analysis.log`

## Compilation and optimization pipeline

The actual pipeline is:

```text
source -> frontend check -> semantic-to-MIR -> SSA conversion
       -> bytecode emission -> Workspace::compile()
       -> Workspace::optimize() [explicit, bytecode-level]
       -> package/runtime
```

`Workspace::compile()` and `Workspace::optimize()` are separate operations.
The CLI `run` and `compile` paths explicitly call both, but the generic
`PackageLoader::load`, the workspace execution API, and several tests call
only `compile()`. Therefore optimized and unoptimized bytecode can currently
be observed by different consumers of the same workspace API.

There are also three MIR optimization implementations that are not called by
the compilation path:

- `compile::inline::inline_functions`
- `compile::inline::optimize_mir`
- `compile::tco::optimize_tail_calls`

They must not simply be enabled. The copy pass uses a function-wide replacement
map without explicit dominance/liveness proof, and the inliner can grow code
aggressively. They need correctness tests and explicit cost limits first.

## Recursive Fibonacci findings

The optimized emitted function is 17 static instructions and declares:

```text
param_count = 1
local_count = 13
temp_count  = 1
frame slots = 15
```

It contains two direct recursive calls. Its relevant shape is:

```text
LoadConst 0; LeI32; JumpTrue
Move n; Jump; Jump-to-next; Ret n
LoadConst 1; SubI32; Move argument; Call fib
LoadConst 2; SubI32; Move argument; Call fib
AddI32; Ret
```

For `fib(35)`, there are 29,860,703 invocations and 14,930,351 non-leaf
invocations. The emitted control path executes about 15 VM instructions per
non-leaf call and 5 per leaf call, before accounting for frame handling. That
is roughly 299 million bytecode dispatches for this benchmark alone. Small
per-call reductions are therefore meaningful.

### Waste in the emitted function

1. **Dead register space from SSA IDs — highest priority.**
   Registers `1..6` are not referenced by the emitted Fibonacci instructions,
   while the function still reserves 13 locals. `image_local_count` uses the
   maximum MIR local ID, not the live bytecode register set. Every recursive
   frame reserves 15 `Value` slots instead of the roughly 8–9 needed by the
   emitted code.

2. **Argument copy before every direct recursive call.**
   The `Call` instruction requires a contiguous argument region, so each
   `n - k` is moved into the single temporary argument register before calling.
   Fibonacci executes this move 29,860,702 times. A `Call1 { arg: Reg }`
   encoding, or an equivalent one-argument direct-call form, removes that move
   and allows the VM to copy directly from the source register.

3. **Constants require a load and typed literal assignments add casts.**
   `fib` loads `1` and `2` from the constant pool on every non-leaf call.
   A small signed immediate variant for integer arithmetic/comparison would
   remove those loads in the recursive hot path. The matrix bytecode also
   reveals `LoadConst; Cast same-register` pairs for typed integer literals;
   those casts are candidates for emission-time elision when the constant type
   already matches the target type.

4. **Redundant CFG layout.**
   The base branch contains a jump whose target is its immediate successor.
   The existing workspace optimizer removes unreachable code and `Move x, x`,
   but neither eliminates jumps-to-next nor threads jumps. The emitter can
   choose a fall-through block layout; a bytecode peephole pass should then
   remove the remaining unconditional jump.

5. **No tail-call opportunity for this Fibonacci formulation.**
   The recursive calls are followed by an addition, so normal tail-call
   elimination is not semantically applicable. An algorithmic rewrite to a
   tail-recursive or iterative Fibonacci implementation would change the
   benchmark's intended recursion workload and must not be used to claim a VM
   recursion improvement.

### VM call-path implications

The local-call fast path is already specialized in `runtime.rs`, avoiding
import resolution. It still checks call depth, manages a frame, copies
arguments, and tracks object anchors. For the all-`i32` Fibonacci case, the
object work is skipped, but frame and register-top bookkeeping remain. The
largest safe improvements are therefore bytecode/register compaction and a
special one-argument call encoding before considering invasive VM changes.

## Matrix findings

The optimized `matrix4` function declares 423 locals and 2 temporaries for
425 frame slots. It contains at least the following static instruction counts:

| Instruction | Count |
| --- | ---: |
| `Move` | 87 |
| `MulI32` | 64 |
| `AddI32` | 52 |
| `LoadConst` | 51 |
| `Cast` | 33 |
| `RemI32` | 16 |
| `Jump` | 4 |

The function has only 314 static instructions, so the local count is not a
source-level variable count: it is primarily the monotonic SSA naming scheme.
This corroborates the Fibonacci observation. Matrix is not recursive, but its
large sparse frame increases register-vector allocation/initialization and
reduces cache locality. It is also a good regression benchmark for register
compaction and typed-constant emission.

## Current bytecode optimizer

The workspace optimizer currently does two jobs:

1. per-function removal of unreachable instructions and `Move { dest == src }`;
2. module pruning/remapping of functions and constants based on exports, init,
   direct calls, and method-name reachability.

It runs after bytecode emission, which is a valid architectural separation:
the compiler can stay incremental and semantic/MIR-aware while the final pass
works on package-level reachability. The issue is not the separation; it is
that the optimizer is currently narrow, costly for its work, and not uniformly
applied by public workspace APIs.

### Optimizer efficiency issues

- `optimize_function` repeats whole-function scans until no change. Each
  changing iteration allocates a removal bitmap and old-to-new map, clones all
  retained instructions, and rewrites every jump. A chain of newly dead blocks
  can make this O(k*n) per function.
- `optimize_package` clones every module before changing it, then rebuilds the
  complete package and graph. It has no changed-module input, so an
  incremental compile still pays a whole-package post-pass when the caller
  opts in.
- Global method reachability is collected as owned `String`s across every
  instruction, then queried with repeated suffix splitting. This is acceptable
  for small packages but allocates and scans more than necessary.
- Pruning retains/remaps functions and constants but does not compact
  registers, remove unused types/layouts, fold constants, simplify branches,
  or remove typed literal casts.
- The optimizer has no idempotency/"nothing changed" fast path, so it creates
  a new package even when every function is already in its fixed point.

### Optimizer correctness and integration risks

- MIR inlining, TCO, and copy propagation are currently dead code. Turning
  them on without a pass manager and regression tests risks bytecode growth or
  invalid replacement across control-flow joins.
- The bytecode optimizer's dynamic/method reachability is necessarily
  conservative. Any more aggressive pruning needs an explicit closed-world
  policy for dynamic calls and adapters.
- `Instruction::CallInternalThread` has opcode `71`, while `decode_opcode`
  currently accepts through `70`. The package currently serializes the enum
  with Postcard rather than this opcode decoder, so this is not evidence of a
  current execution failure; it is nevertheless an ABI validation inconsistency
  that should be fixed and tested independently.

## Recommended implementation order

### Phase A — Make optimization explicit and observable

1. Define a compile profile or workspace option that consistently selects
   bytecode optimization for CLI, hosts, and programmatic execution.
2. Add bytecode metrics per function: instruction count, register count,
   constant count, and before/after deltas. Keep them behind the existing
   metrics/debug facility.
3. Add bytecode golden tests for Fibonacci, a loop, branches, dynamic calls,
   providers, and thread intrinsics.

Success criterion: the chosen profile always produces the same optimized
package regardless of entry API.

### Phase B — Safe bytecode canonicalization

1. Add a single CFG-aware reachability/layout pass instead of iterative
   full-vector cloning.
2. Remove jumps to the next instruction and thread chains of unconditional
   jumps; then rebuild relative offsets once.
3. Elide `Cast { dest, src: dest }` only when the producer's type proves the
   cast is identity.
4. Add an unchanged-package fast path and avoid cloning modules whose bytecode
   did not change.

Success criterion: smaller bytecode with no semantic changes; Fibonacci loses
the redundant base-path jump.

### Phase C — Register compaction (highest expected general gain)

1. After emission and CFG cleanup, compute live register intervals for each
   function and remap non-parameter registers densely.
2. Preserve contiguous windows required by calls, aggregate construction, and
   future instructions; reserve/reuse a compact temporary area.
3. Rewrite all register operands and set `local_count`/`temp_count` from the
   new allocation.

Start with a simple dense "used register" remap, which already removes the
proven holes in Fibonacci. Follow with liveness-based reuse only after
correctness coverage for ownership, branches, awaits, and parallel copies.

Success criterion: Fibonacci frames drop from 15 slots to the live minimum;
matrix's 425-slot frame materially shrinks.

### Phase D — Recursive call and literal specialization

1. Add compact typed immediate forms such as `SubI32Imm` and `LeI32Imm` for
   small signed integers, only after measuring code-size and dispatch effects.
2. Add a one-argument direct-call form that accepts an arbitrary argument
   register, avoiding the emitter's argument `Move` and the temporary window.
3. Keep general `Call` for N arguments and cross-module calls until an import
   equivalent is proven worthwhile.

Success criterion: Fibonacci's two recursive sites no longer need constant
loads or argument moves, while its result, error behavior, call-depth limit,
and ownership behavior remain unchanged.

### Phase E — MIR pass manager, not blind activation

1. Establish a MIR pass pipeline before SSA-sensitive bytecode emission with
   explicit invariants and per-pass verification.
2. Replace the current global copy map with dominance-aware local propagation
   over SSA definitions.
3. Enable only size-bounded, non-recursive, non-async inlining after measuring
   a threshold; do not inline recursive functions.
4. Enable TCO only for proven tail positions. It benefits tail-recursive code,
   not the current Fibonacci tree recursion.

Success criterion: MIR passes reduce instructions or calls on targeted tests
without code-size explosion or changes in ownership/async semantics.

## Measurement plan

For every phase, record release, no-metrics medians for `fib`, `matrix4`, and
`tasks`, plus metrics builds for attribution. Add these bytecode figures:

```text
per function: static instructions, live registers, temp registers,
constants referenced, direct calls, dynamic calls, branch count,
before/after optimizer delta
```

For Fibonacci, additionally count VM instruction dispatches, local calls,
argument moves, and maximum frame slots. A change should be judged first by
the reduction in these counters, then by multi-sample wall time versus QuickJS.

## Priority summary

| Priority | Work | Expected effect |
| --- | --- | --- |
| P0 | Consistent optimized profile + bytecode tests | Prevents misleading comparisons and protects later changes |
| P1 | Safe bytecode CFG canonicalization | Small immediate size/dispatch reduction |
| P1 | Register compaction | Large frame/cache reduction in recursion and matrix |
| P2 | Typed literal and one-argument call forms | Major Fibonacci hot-path instruction reduction |
| P2 | Incremental/no-change optimizer path | Faster compilation and editor workflow |
| P3 | Verified MIR pass manager/inlining/TCO | Broader gains with higher correctness risk |
