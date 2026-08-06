Previous: [Iteration and Ranges](./12-iteration-and-ranges.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Decorators and Keyword Metadata](./14-decorators-and-keyword-metadata.md)

---

# 13. Pattern Matching and Narrowing

This document defines `match`, `instanceof`, patterns, wildcard rules, exhaustiveness, narrowing, pattern bindings, and unreachable pattern diagnostics.

## 13.1 Core Principle

Galfus has one narrowing expression:

```txt
instanceof
```

`match` handles value/data-pattern matching.

`instanceof` handles type, null, union, generic known-set, nominal, and facade narrowing.

## 13.2 Match

```galfus
var label = match value {
  0 => "zero",
  1 => "one",
  _ => "many",
}
```

`match` is value-producing and must be exhaustive.

All arms must produce compatible result types.

Arms are comma-separated. The comma is required between arms even when an arm body is a block.

An expression arm uses `=>`. A block arm follows its pattern directly; blocks do not implicitly yield their last expression.

```galfus
fn label(result: Result): i32 {
  return match result {
    Result::Ok(value) => value,
    Result::Err(error) {
      report(error)
      return 0
    },
  }
}
```

## 13.3 Wildcard Arm Position

If `_` appears in an arm list, it must be the final arm.

Valid:

```galfus
match value {
  0 => "zero",
  _ => "other",
}
```

Invalid:

```galfus
match value {
  _ => "other",
  0 => "zero",
}
```

Wildcard before the final arm is an error, not a warning.

The same rule applies to `instanceof`.

## 13.4 Pattern Bindings Are Const

Pattern bindings inside `match` and `instanceof` are constant by default.

```galfus
fn inspect(result: Result): i32 {
  return match result {
    Result::Ok(value) {
      value = 10 // invalid
      return value
    },
    Result::Err(error) => 0,
  }
}
```

Complex pattern bindings share the matched graph. They are not deep-copied.

## 13.5 Literal Patterns

```galfus
match count {
  0 => "zero",
  1 => "one",
  _ => "many",
}
```

Supported literal patterns:

```txt
integer
float
bool
null
string as [u8]
```

## 13.6 Enum Patterns

```galfus
var label = match direction {
  Direction::North => "north",
  Direction::East => "east",
  Direction::South => "south",
  Direction::West => "west",
}
```

All variants must be handled unless `_` is present.

## 13.7 Choice Patterns

```galfus
var value = match result {
  Result::Ok(value) => value,
  Result::Err(error) => 0,
}
```

No-payload variants are matched without parentheses.

```galfus
Maybe::None => 0
```

Payload arity must match the variant declaration.

## 13.8 Tuple and Array Patterns

Tuple patterns:

```galfus
match point {
  (0, 0) => "origin",
  (x, y) => "point",
}
```

Array patterns match elements and lengths of dynamic arrays at runtime.

```galfus
match pair {
  [first, second] => first + second,
}
```

Runtime-sized array destructuring can be added later if needed.

## 13.9 Struct Patterns

```galfus
match user {
  User { id, name } => name,
}
```

Omitted fields are ignored.

```galfus
match user {
  User { name } => name,
}
```

Struct destructuring does not use `_` for omitted fields.

Field rename:

```galfus
match user {
  User { name: displayName } => displayName,
}
```

## 13.10 `instanceof`

`instanceof` is the value narrowing expression.

It handles:

```txt
primitive narrowing
null narrowing
union narrowing
generic narrowing with known type set
nominal struct narrowing
facade/concrete narrowing
```

Example:

```galfus
var size = instanceof value {
  i32 number => number,
  [u8] text => text.length,
  null => 0,
}
```

Fallback binding:

```galfus
fn narrow(entry: i32 | i16): i32 {
  return instanceof entry {
    i32 x => 1,
    i16 x {
      return 2
    },
    x => 0,
  }
}
```

The final `x =>` is a catch-all binding for the remaining possible type set.

Wildcard fallback:

```galfus
var value = instanceof entry {
  i32 x => 1,
  _ => 0,
}
```

`_ =>` is a catch-all for the remaining possible type set without creating a binding.

Invalid subject:

```galfus
instanceof _ {
  _ => 0,
}
```

`instanceof` requires an input expression. `_` may appear in arms, but not as the input.

Nullable narrowing:

```galfus
var name = instanceof user {
  User value => value.name,
  null => "missing",
}
```

Facade narrowing:

```galfus
var label = instanceof item {
  Button button => button.label,
  Icon icon => icon.name,
  _ => "unknown",
}
```

## 13.11 `instanceof` and Generics

Valid only when possible runtime type set is statically known.

```galfus
fn render<T: i32 | [u8] | bool>(value: T): [u8] {
  return instanceof value {
    i32 number => "number",
    [u8] text => text,
    _ => "other",
  }
}
```

Invalid over unconstrained `T`.

```galfus
fn render<T>(value: T): [u8] {
  return instanceof value {
    i32 number => "number",
    _ => "other",
  }
}
```

## 13.12 `typeof`

`typeof` dispatches over a type expression.

```galfus
var result = typeof T {
  i8 => parseInt8(bytes),
  u8 => parseUint8(bytes),
  bool => parseBool(bytes),
  _ => null,
}
```

`typeof` arms are type arms. They do not bind values.

`_ =>` is a wildcard for the remaining possible type set and must be final.

For bounded generic input, the expression must cover all members of the bound or include a final `_`.

```galfus
fn parse<T: i8 | u8 | bool>(bytes: [u8]): T | null {
  return typeof T {
    i8 => parseInt8(bytes),
    u8 => parseUint8(bytes),
    bool => parseBool(bytes),
  }
}
```

For unconstrained generic input, `_` is required.

```galfus
fn parse<T>(bytes: [u8]): T | null {
  return typeof T {
    _ => null,
  }
}
```

Inside each concrete arm, the checker narrows the input generic to the arm type for expected type propagation.

## 13.13 Destructuring Bindings

Patterns may be used in destructuring bindings.

```galfus
var (x, y) = point
var [first, second] = pair
var User { name } = user
```

Outer binding mutability controls created bindings.

```galfus
var (x, y) = point
const (a, b) = point
```

This differs from match/instanceof bindings, which are const by default.

## 13.14 No Pattern Guards

Pattern guards are not part of Galfus.

Invalid:

```galfus
match value {
  x if x > 10 => "large",
  _ => "small",
}
```

Custom conditional matching should be expressed through explicit comparable behavior or normal code inside an arm.

## 13.15 Comparable Matching

For custom value comparison patterns, the type must provide deterministic comparable behavior.

If source references `Comparable`, it must import it.

```galfus
import { Comparable } from "std/constraints"
```

Operators are still not overloaded.

## 13.16 Unreachable Patterns

Unreachable non-wildcard patterns are warnings.

```galfus
match value {
  0 => "zero",
  0 => "again",
  _ => "other",
}
```

The second `0` is unreachable.

Wildcard before the final arm is an error.

## 13.17 Contract

The checker MUST:

- Use `instanceof` for all narrowing.
- Require `match` and `instanceof` expressions to be exhaustive.
- Require `_` to be the final arm when present.
- Treat wildcard-before-final as error.
- Treat unreachable non-wildcard patterns as warnings.
- Treat pattern bindings in arms as const.
- Reject pattern guards.
- Reject impossible type/narrowing arms.
- Avoid implicit deep copy during pattern binding.

---

Previous: [Iteration and Ranges](./12-iteration-and-ranges.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Decorators and Keyword Metadata](./14-decorators-and-keyword-metadata.md)
