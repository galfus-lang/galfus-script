Previous: [Pattern Matching and Narrowing](./13-pattern-matching-and-narrowing.md) | Index: [Galfus Core System](./00-index.md) | Next: [Lowering and Runtime Semantics](./15-lowering-and-runtime-semantics.md)

---

# 14. Decorators and Keyword Metadata

This document defines decorators and keyword metadata. They are separate mechanisms.

## 14.1 Core Principle

Decorators are not metadata.

A decorator is a typed compile-time transformer function.

Keyword metadata is syntax-owned static information attached directly to a language keyword.

Decorator:

```galfus
@trace("sum")
fn sum(a: i32, b: i32): i32 {
  return a + b
}
```

Keyword metadata:

```galfus
fn(stamp) max(a: i32, b: i32): i32 {
  return a
}
```

## 14.2 Decorator Type Rule

A function can be used as a decorator only if:

```txt
first parameter type == decorated target type
return type == decorated target type
remaining parameters == decorator arguments
```

Conceptual form:

```txt
Decorator<T, Args...> = fn(T, Args...): T
```

The decorator receives the target and returns a transformed target of the same type.

## 14.3 Decorator Application

```galfus
@trace("sum")
fn sum(a: i32, b: i32): i32 {
  return a + b
}
```

Conceptual application:

```txt
sum = trace(sum, "sum")
```

The exact implementation may be compiler-level, but type rules must hold.

## 14.4 Decorator Order

Decorators may repeat freely.

Application order is from closest to the target to farthest.

```galfus
@outer
@middle
@inner
fn call(): null {
}
```

Application:

```txt
call1 = inner(call)
call2 = middle(call1)
call3 = outer(call2)
```

Equivalent:

```txt
outer(middle(inner(call)))
```

## 14.5 Allowed Decorator Targets

Decorators may attach to:

```txt
function declarations
function parameters
struct declarations
struct fields
choice variant tuple fields
```

Decorators cannot attach to:

```txt
enum declarations
enum variants
choice declarations as a whole
choice variants as a whole
stamped functions
```

## 14.6 Function Decorators

A function decorator transforms a function while preserving its function type.

```galfus
@trace("sum")
fn sum(a: i32, b: i32): i32 {
  return a + b
}
```

The decorator must accept and return:

```txt
fn(i32, i32): i32
```

It cannot change the function signature.

## 14.7 Parameter Decorators

```galfus
fn route(@normalize path: [u8]): null {
}
```

A parameter decorator must preserve the parameter type.

Parameter decorators cannot change the fact that parameters are const bindings by default.

## 14.8 Struct Decorators

```galfus
@deriveDebug
struct User {
  id: i64,
  name: [u8],
}
```

A struct decorator transforms the struct target while preserving the same struct type.

It may generate associated behavior, but it must not turn the struct into another type.

## 14.9 Struct Field Decorators

```galfus
struct User {
  @clamp(0, 120)
  age: i32,
}
```

The decorator must preserve the field type.

It cannot silently change mutability or ownership rules.

## 14.10 Choice Payload Field Decorators

Decorators may attach to tuple fields inside choice variant payloads.

```galfus
choice Result<V, E> {
  Ok(@validate V),
  Err(@normalize E),
}
```

Invalid on choice declaration:

```galfus
@tagged
choice Result<V, E> {
  Ok(V),
  Err(E),
}
```

Invalid on choice variant as a whole:

```galfus
choice Result<V, E> {
  @success
  Ok(V),
  Err(E),
}
```

## 14.11 Enum Decorators Are Invalid

Invalid:

```galfus
@repr("c")
enum(u8) Kind {
  A(1),
}
```

Enum representation uses keyword metadata.

```galfus
enum(u8) Kind {
  A(1),
}
```

## 14.12 Stamped Functions Cannot Receive Decorators

Invalid:

```galfus
@trace("max")
fn(stamp) max(a: i32, b: i32): i32 {
  return a
}
```

Reason:

```txt
fn(stamp) changes lowering behavior
decorators transform function targets
combining them creates ambiguous transform/lowering ordering
```

## 14.13 Keyword Metadata

Keyword metadata is separate from decorators.

Compiler-known metadata:

```txt
fn(stamp)
fn(async)
loop(after)
loop(name: ...)
for(name: ...)
enum(IntegerType)
```

Examples:

```galfus
fn(async) loadData(): [u8] {
}

loop(after) ready {
}

loop(name: root) {
}

for(name: users) user in users {
}

enum(i64) BigKind {
  A(1),
}
```

Metadata names are not globally reserved.

## 14.14 Invalid Metadata Position

Metadata is valid only in supported positions.

Invalid:

```galfus
struct(stamp) User {
}

```

## 14.15 Determinism

Decorators and keyword metadata must be deterministic.

Decorator arguments SHOULD be compile-time known values.

Invalid:

```galfus
@route(readRuntimeRoute())
fn call(): null {
}
```

Valid:

```galfus
@route("/users")
fn call(): null {
}
```

## 14.16 Contract

The checker/lowering MUST:

- Treat decorators as transformer functions, not metadata.
- Validate decorator first parameter and return type.
- Allow repeated decorators.
- Apply decorators closest-to-farthest from the target.
- Reject decorators on enums, choice declarations, whole choice variants, and stamped functions.
- Reject decorators that change target type.
- Keep keyword metadata separate from decorators.
- Validate keyword metadata by syntax position.
- Keep metadata names non-reserved outside metadata positions.

---

Previous: [Pattern Matching and Narrowing](./13-pattern-matching-and-narrowing.md) | Index: [Galfus Core System](./00-index.md) | Next: [Lowering and Runtime Semantics](./15-lowering-and-runtime-semantics.md)
