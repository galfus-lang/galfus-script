Previous: [Type System, Inference and Propagation](./04-type-system-inference-and-propagation.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Constraints as Traits](./06-constraints-as-traits.md)

---

# 5. Generics

This document defines static generic parameters, generic functions, generic data forms, generic constraints, inference, explicit generic arguments, and lowering expectations.

## 5.1 Core Principle

Galfus generics are static.

They are closer to Rust-style generics than dynamic generics.

There is no dynamic `any`, `unknown`, or runtime fallback type.

## 5.2 Generic Parameters

Generic parameters use angle brackets.

```galfus
fn identity<T>(value: T): T {
  return value
}
```

Generic parameters are local to the declaration.

Duplicate generic parameter names are invalid.

```galfus
fn invalid<T, T>(value: T): T {
  return value
}
```

## 5.3 Generic Functions

```galfus
fn first<T>(value: T): T {
  return value
}

var a = first(10)
var b = first("Ana")
```

Generic arguments may be inferred from call arguments and expected return type.

## 5.4 Explicit Generic Arguments

Explicit generic call syntax:

```galfus
identity<i64>(10)
math::max<i64>(a, b)
```

Initial rule:

```txt
explicit generic argument lists provide all generic arguments or none
```

Partial explicit generic arguments are not supported initially.

## 5.5 Generic Structs

```galfus
struct Box<T> {
  value: T,
}

var box = new(Box<i32>) {
  value: 10,
}
```

`Box<i32>` and `Box<i64>` are distinct concrete types.

There is no implicit conversion between them.

Generic types are invariant by default.

## 5.6 Generic Choices

```galfus
choice Result<V, E> {
  Ok(V),
  Err(E),
}
```

Expected type may infer generic arguments.

```galfus
var result: Result<i32, [u8]> = Result::Ok(10)
```

Invalid when generic arguments cannot be fully inferred:

```galfus
var result = Result::Err("failed")
```

`V` is unknown.

## 5.7 Generic Constraints

Constraints are explicit. Builtin constraints are compiler-known but not globally imported.

If source code mentions them, it must import them.

```galfus
import { Comparable, Iterable, Iterator } from "std/constraints"
```

Generic bound example:

```galfus
fn min<T: Comparable<T>>(a: T, b: T): T {
  if a::compare(b) <= 0 {
    return a
  }

  return b
}
```

The `<=` operator applies to the primitive result of `compare`, not to `T`.

## 5.8 No Operator Unlocking

Constraints do not unlock or overload operators.

Invalid:

```galfus
constraint Addable<T> {
  fn add(self, other: T): T
}

fn add<T: Addable<T>>(a: T, b: T): T {
  return a + b
}
```

Valid:

```galfus
fn add<T: Addable<T>>(a: T, b: T): T {
  return a::add(b)
}
```

## 5.9 Generic Literals and Struct Construction

Generic struct literals can infer from expected type or field values.

```galfus
var box: Box<i32> = new(Box) {
  value: 10,
}
```

Invalid when inference is ambiguous:

```galfus
var box = new(Box) {
  value: null,
}
```

## 5.10 Generic Arrays

Generic array forms:

```galfus
[T]
```

## 5.11 `instanceof` and `typeof` with Generics

`instanceof` may narrow generic values only when the possible runtime type set is statically known.

Valid:

```galfus
fn render<T: i32 | [u8] | bool>(value: T): [u8] {
  return instanceof value {
    i32 number => "number",
    [u8] text => text,
    _ => "other",
  }
}
```

Invalid:

```galfus
fn render<T>(value: T): [u8] {
  return instanceof value {
    i32 number => "number",
    _ => "other",
  }
}
```

There is no implicit unknown fallback.

`typeof` may dispatch on a generic type parameter.

```galfus
fn parse<T: i8 | u8 | bool>(s: [u8]): T | null {
  return typeof T {
    i8 => parseInt8(s),
    u8 => parseUint8(s),
    bool => parseBool(s),
  }
}
```

For a bounded generic, `typeof` must be exhaustive over the bound or end with `_`.

```galfus
fn parse<T: i8 | u8 | bool>(s: [u8]): T | null {
  return typeof T {
    i8 => parseInt8(s),
    u8 => parseUint8(s),
    _ => null,
  }
}
```

For an unconstrained generic, `typeof` must include a final `_` arm because the checker has no closed type set.

```galfus
fn parse<T>(s: [u8]): T | null {
  return typeof T {
    _ => null,
  }
}
```

Inside each concrete `typeof` arm, expected type propagation treats the matched generic as that arm type.

## 5.12 Generic Recursion

Generic recursion is valid only when instantiation remains bounded.

Invalid unbounded chain example:

```txt
A<T> -> A<Box<T>> -> A<Box<Box<T>>> -> ...
```

Stamped generic functions must also avoid unbounded instantiation.

## 5.13 Generic Lowering

Generic lowering may use:

```txt
monomorphization
shared templates
hybrid strategy
```

Observable semantics MUST remain static and deterministic.

## 5.14 Contract

The compiler MUST:

- Keep generics static.
- Reject partial explicit generic arguments initially.
- Treat generic types as invariant by default.
- Reject unconstrained `instanceof` over generic `T`.
- Reject unbounded generic instantiation chains.
- Avoid dynamic `any` or `unknown` behavior.
- Require explicit imports when users refer to builtin constraint names in source.

---

Previous: [Type System, Inference and Propagation](./04-type-system-inference-and-propagation.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Constraints as Traits](./06-constraints-as-traits.md)
