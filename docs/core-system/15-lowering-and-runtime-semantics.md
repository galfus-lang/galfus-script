Previous: [Decorators and Keyword Metadata](./14-decorators-and-keyword-metadata.md) | Index: [Galfus Core System](./00-index.md)

---

# 15. Lowering and Runtime Semantics

This document defines how validated Galfus source semantics lower into the
current in-memory runtime representation. Bundle generation is planned and is
not part of the implemented pipeline.

## 15.1 Core Principle

Lowering MUST preserve source semantics exactly.

No lowering step may introduce:

```txt
hidden dynamic typing
implicit deep copy
implicit operator overloading
implicit global imports
implicit function return inference
runtime-dependent compilation behavior
```

## 15.2 Compilation Pipeline

Recommended high-level pipeline:

```txt
1. lexing
2. parsing
3. local symbol collection
4. module/import resolution
5. type resolution
6. generic resolution
7. constraint validation
8. decorator validation and application
9. keyword metadata validation
10. expression/type checking
11. ownership validation
12. lowering to `BytecodeModule`
13. assembly into an in-memory `BytecodeGraph`
```

Implementation may split or merge phases, but output must be deterministic.

## 15.3 Artifact Model

Public/source-level file types:

```txt
.gfs -> Galfus source
.gfp -> Galfus proxy/adaptor definition
```

`BytecodeModule` is the internal compiler/runtime representation. It is held
in memory as a node of a `BytecodeGraph`; no bytecode serialization or bundle
format is implemented.

Conceptual pipeline:

```txt
.gfs
  -> SemanticGraph
  -> BytecodeModule
  -> BytecodeGraph
```

Optional execution metadata may map bytecode instructions to source spans.

## 15.4 Module Lowering

Each `.gfs` source module lowers with:

```txt
resolved imports
private declarations
exported declarations
local symbol table
type information
function bodies
ownership metadata
decorator-transformed declarations
keyword metadata
```

Only exported symbols become part of the module surface.

Unused private symbols MAY be removed if observable behavior is unchanged.

## 15.5 Decorator Lowering

Decorators are transformer functions.

They apply closest-to-farthest.

```galfus
@outer
@inner
fn call(): null {
}
```

Conceptual:

```txt
call1 = inner(call)
call2 = outer(call1)
```

Decorators may repeat freely.

Stamped functions cannot receive decorators.

## 15.6 Keyword Metadata Lowering

Compiler-known keyword metadata:

```txt
fn(stamp) -> callsite expansion/lowering-time specialization candidate
loop(after) -> condition-after loop shape
loop(name: X) -> named loop target
for(name: X) -> named for target
enum(T) -> enum discriminant representation
```

Invalid metadata positions are rejected before lowering.

## 15.7 Function Lowering

Every function has explicit return type.

Function parameters lower as constant local bindings.

For return types accepting `null`, fallthrough lowers to `return null`.

Non-null functions require definite return validation before lowering.

Stamped functions must be lowerable inline and cannot be recursive or decorated.

## 15.8 Call Lowering

Call evaluation order:

```txt
target first
arguments left to right
invoke target
```

Trailing defaults may be omitted.

Middle defaults use `_`.

Argument gaps are invalid.

## 15.9 Anchor Call Lowering

```galfus
user::rename("Ana")
```

Conceptual lowering:

```txt
User::rename(user, "Ana")
```

No implicit write-back occurs.

## 15.10 Primitive and Complex Values

Primitive scalar values copy by value.

Complex values are reachable value graphs.

Complex assignment shares the graph.

```galfus
var other = user
```

Deep copy requires:

```galfus
var other = copy user
```

## 15.11 Ownership Lowering

Ownership lowering tracks:

```txt
anchors
edges
weak observers
release points
copy eligibility
resource policies
```

Fields, array elements, tuple elements, and choice payloads may create edges.

Weak fields create weak observers.

## 15.12 Assignment Lowering

Assignment is statement-only.

Compound assignment lowers to operation plus assignment.

```galfus
count += 1
```

Conceptual:

```txt
tmp = count + 1
count = tmp
```

Fallback assignment lowers to a null check.

```galfus
cache ??= createCache()
```

## 15.13 Copy Lowering

`copy` must:

```txt
duplicate owning topology
preserve ownership topology inside the copy
validate weak observers
respect resource policies
reject non-copyable values
```

Weak observers are not promoted into owning edges.

Fieldless structs are rejected for `copy`.

## 15.14 Release Points

Graphs are released at deterministic safe points.

Recommended safe points:

```txt
end of statement
after reassignment
end of block
function return
safe loop iteration boundary
```

Weak observers become `null` when their target is released.

## 15.15 Arrays and Strings

Arrays lower to array values.

Array construction:

```galfus
new([i32], 4)
```

Arrays expose only:

```galfus
values.length
```

Out-of-bounds read returns `null`.

Out-of-bounds write is a runtime error.

String literals lower to UTF-8 `[u8]` arrays.

## 15.16 Enums and Choices

Enums lower to nominal discriminant values.

Default enum base type is `i32`.

Explicit base type uses `enum(T)`.

Choices lower to tagged unions with optional payloads.

Payloads participate in the ownership graph.

## 15.17 Pattern and Narrowing Lowering

`match` lowers to deterministic pattern selection.

`instanceof` is the value narrowing expression.

It handles:

```txt
primitive narrowing
null narrowing
union narrowing
known generic type-set narrowing
nominal struct narrowing
facade/concrete narrowing
```

`_` must be final when present.

Fallback bindings consume the remaining possible type set and bind that remaining type.

Wildcard fallback consumes the remaining possible type set without creating a binding.

The `instanceof` subject must be a real input expression. `_` is invalid as the subject.

Unreachable non-wildcard patterns are warnings.

`typeof` lowers to deterministic type-set dispatch.

`typeof` over a bounded generic must be exhaustive over the bound or provide a final `_` wildcard arm.

`typeof` over an unconstrained generic must provide a final `_` wildcard arm.

## 15.18 Loop and For Lowering

`loop` forms:

```txt
loop { body }
loop condition { body }
loop(after) condition { body }
```

Named loops and named `for` lower to control-flow targets.

`for` source is evaluated once.

Item and index bindings are const.

## 15.19 Range Lowering

Ranges are compiler-known literal iterables.

```txt
start..end
start::count
start::count%step
```

`start..end` lowers conceptually through `std/range::range(start, end)` and has type `RangeExclusive`.

`start::count` and `start::count%step` lower conceptually through `std/range::rangeSteps(start, count, step)` and have type `RangeStepped<i64>` or `RangeStepped<f64>`. `count` is always an integer literal. If `start` or `step` is a float literal, integer literals are promoted and the stepped range uses `f64`.

Invalid:

```galfus
a..b
1..1
1::0
1::-1
1::2%0
1.0..2.0
1::4%0.5
```

Ranges do not materialize arrays.

## 15.20 Constraint Facade Lowering

Constraint facade values expose only constraint-required fields and functions.

Dispatch may use:

```txt
static direct call
compact dispatch metadata
vtable-like representation
```

Observable behavior must remain deterministic.

## 15.21 Generic Lowering

Generic lowering may use monomorphization, shared templates, or a hybrid strategy.

Observable semantics must remain static.

No dynamic `any` or `unknown` behavior is allowed.

## 15.22 Operator Lowering

Operators have fixed meanings.

No operator overloading exists.

Custom behavior lowers through named calls.

## 15.23 Warnings and Errors

Warnings:

```txt
unreachable code
unreachable non-wildcard pattern
unused private declaration
unused local binding
unused import
```

Errors:

```txt
unknown symbol
unknown import
type mismatch
non-exhaustive match
invalid instanceof arm
invalid wildcard position
invalid decorator target/type
invalid metadata target
invalid assignment target
copy of fieldless struct
copy of non-copyable resource
weak field without nullable type
function missing return type
non-null function fallthrough
argument gap
```

## 15.24 Reproducibility

Same source and same dependencies MUST produce the same compilation result.

The compiler SHOULD keep deterministic ordering for:

```txt
symbol tables
module traversal
diagnostics
exports
generated IDs
```

## 15.25 Contract

Lowering MUST:

- Preserve validated source semantics.
- Produce in-memory `BytecodeModule` values for the `BytecodeGraph`.
- Apply decorators closest-to-farthest.
- Reject decorators on stamped functions.
- Keep keyword metadata separate from decorators.
- Treat function parameters and `for` bindings as const.
- Share complex assignment graphs.
- Lower `copy` as explicit deep copy with weak validation.
- Reject `copy` of fieldless structs.
- Use `instanceof` for value narrowing and `typeof` for type dispatch.
- Keep ranges as integer literal iterables.
- Preserve reproducible builds.

---

Previous: [Decorators and Keyword Metadata](./14-decorators-and-keyword-metadata.md) | Index: [Galfus Core System](./00-index.md)
