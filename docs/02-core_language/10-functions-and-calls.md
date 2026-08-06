Previous: [Mutation, Assignment and Ownership](./09-mutation-assignment-and-ownership.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Control Flow](./11-control-flow.md)

---

# 10. Functions and Calls

This document defines function declarations, return types, expression and block bodies, parameters, defaults, rest parameters, calls, anchor functions, stamped functions, closures, and function types.

## 10.1 Function Declaration

```galfus
fn sum(a: i32, b: i32): i32 {
  return a + b
}
```

A function body is either one expression introduced by `=>` or a direct block.

```galfus
fn sum(a: i32, b: i32): i32 => a + b
```

Block bodies do not implicitly return their last expression. Use `return` for every value-producing path.

Every function MUST declare a return type.

Invalid:

```galfus
fn call() {
}
```

Valid:

```galfus
fn call(): null {
}
```

## 10.2 Return Behavior

A function whose return type includes `null` may fall through.

Equivalent:

```galfus
fn call(): null {
}
```

```galfus
fn call(): null {
  return
}
```

```galfus
fn call(): null {
  return null
}
```

A non-null function must return a compatible value on all reachable paths.

Invalid:

```galfus
fn value(): i32 {
}
```

## 10.3 Parameters Are Const Bindings

Function parameters are constant bindings by default.

```galfus
fn call(value: i32): null {
  value = 10 // invalid
}
```

Complex parameters share the same graph, but the parameter binding cannot be rebound.

```galfus
fn rename(user: User, name: [u8]): null {
  user.name = name // valid if field is mutable
}
```

## 10.4 Default Parameters

```galfus
fn connect(host: [u8], port: i32 = 80): bool {
  return true
}

connect("localhost")
connect("localhost", 8080)
```

Trailing default parameters may be omitted.

Middle default parameters require `_`.

```galfus
fn call(a: i32, b: i32 = 2, c: i32 = 3): null {
}

call(1)
call(1, _)
call(1, _, 3)
```

Argument gaps are invalid.

```galfus
call(1, _, 3) // valid: b uses its default
call(1,,3)    // invalid: omitted middle defaults require `_`
```

## 10.5 Rest Parameters

A rest parameter receives a runtime-sized sequence of positional arguments.

```galfus
fn summarize(...values: [i32]): i32 {
  var total = 0

  for value in values {
    total += value
  }

  return total
}
```

A function may have at most one rest parameter.

The rest parameter SHOULD be final.

Function calls do not support argument spread initially.

Invalid:

```galfus
summarize(...values)
```

## 10.6 Calls

Call checking validates:

```txt
target is callable
argument count
default mapping
rest mapping
argument types
return type
generic inference
```

Arguments evaluate left to right after the target is resolved/evaluated.

## 10.7 Anchor Functions

```galfus
fn User::rename(self, name: [u8]): User {
  self.name = name
  return self
}
```

`self` has no explicit type annotation.

Anchor call:

```galfus
user::rename("Ana")
```

There is no implicit write-back.

```galfus
user = user::rename("Ana")
```

## 10.8 Stamped Functions

Stamped functions use keyword metadata.

```galfus
fn(stamp) max(a: i32, b: i32): i32 {
  if a > b {
    return a
  }

  return b
}
```

Stamped functions are eligible for callsite expansion or lowering-time specialization.

Restrictions:

```txt
no direct recursion
no indirect stamped recursion
no unbounded generic instantiation
must be lowerable inline
cannot receive decorators
```

Invalid:

```galfus
@trace("max")
fn(stamp) max(a: i32, b: i32): i32 {
  return a
}
```

## 10.9 Generic Functions

```galfus
fn identity<T>(value: T): T {
  return value
}

identity<i64>(10)
```

Explicit generic argument lists must be complete or omitted initially.

## 10.10 Function Expressions and Closures

Expression body:

```galfus
var double = fn (value: i32): i32 => value * 2
```

Block body:

```galfus
var double = fn (value: i32): i32 {
  return value * 2
}
```

Function expressions may capture surrounding values.

Captured complex values participate in ownership.

## 10.11 Function Types

```galfus
fn(i32, i32): i32
fn([u8]): null
```

Named functions may be used as values.

```galfus
var operation: fn(i32, i32): i32 = sum
```

Function types are invariant by default.

## 10.12 No Function Overloading

Function overloading by parameter type is not supported.

Invalid:

```galfus
fn format(value: i32): [u8] {
  return "int"
}

fn format(value: bool): [u8] {
  return "bool"
}
```

Use distinct names or constraint-based behavior.

## 10.13 Asynchronous Functions

Asynchronous functions are declared with the `async` keyword metadata: `fn(async)`.

```galfus
fn(async) fetchUser(id: i64): User {
  const user = await loadUser(id)
  return user
}
```

Anonymous function expressions may also be marked as `async`:

```galfus
const load = fn(async) (id: i64): User => await loadUser(id)
```

Async functions allow the use of `await` expressions within their body.

The return type annotation of an `fn(async)` function specifies the inner payload type (e.g. `: User` or `: [u8]`), NOT `Future<T>`. Applying the `fn(async)` metadata automatically wraps the return value in `Future<T>` for callers.

## 10.14 Contract

The checker MUST:

- Require explicit return type.
- Treat parameters as const bindings.
- Allow fallthrough only when return type accepts `null`.
- Reject missing return in non-null functions.
- Support trailing omitted defaults.
- Support `_` for middle default arguments.
- Reject argument gaps.
- Reject decorators on stamped functions.
- Reject stamped recursion.
- Reject function overloads by signature.
- Support `fn(async)` metadata for asynchronous function declarations and expressions.

---

Previous: [Mutation, Assignment and Ownership](./09-mutation-assignment-and-ownership.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Control Flow](./11-control-flow.md)
