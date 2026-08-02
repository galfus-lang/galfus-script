---
name: galfus-script
description: Comprehensive guide and rules for writing valid Galfus Script code (.gfs), covering syntax, types, data forms, control flow, functions, async/await, builtins, and project patterns.
---

# Galfus Script Developer Skill

Use this skill whenever you need to read, write, refactor, or debug **Galfus Script** (`.gfs`) code.

---

## 1. Quick Syntax & Feature Cheat Sheet

| Feature | Galfus Script Syntax | Common Mistake / Pitfall |
| :--- | :--- | :--- |
| **Variable Binding** | `var x = 1` (mutable), `const y = 2` (immutable) | Using `let` or omitting type for uninitialized `var x: i32` |
| **Strings** | `"Hello"` (type is UTF-8 `[u8]`) | Assuming `String` type exists (Galfus uses `[u8]`) |
| **Arrays** | `[1, 2, 3]`, creation: `new([i32], 10)` | Assuming `.push()`/`.pop()` exist (only `.length` is built-in) |
| **Methods** | Anchor: `fn Struct::method(self): T`, Call: `obj::method()` | Calling method with `.` instead of `::` (`obj.method()` is wrong) |
| **Functions** | `fn name(a: i32): i32 { return a }` | Omitting return type (return type is **mandatory**) |
| **Async Functions** | `fn(async) fetch(id: i64): User { ... }` | Writing `async fn` (Galfus uses keyword metadata `fn(async)`) |
| **Single Await** | `const res = await load(id)` | Writing `await()` without target |
| **Await All** | `const (a, b) = await(all) (loadA(), loadB())` | Using `Promise.all` or non-tuple forms |
| **Await Race** | `const winner = await(race) (loadA(), loadB())` | Using `Promise.race` |
| **Loops** | `loop { ... }`, `for i in 0..10`, `loop(name: id)` | Writing `while` or `do` (they do **NOT** exist) |
| **Errors** | `choice Result<V, E> { Ok(V), Err(E) }` | Using `try`, `catch`, or `throw` (they do **NOT** exist) |
| **Deep Copy** | `var cloned = copy user` | Assuming assignment deep-copies (assignment passes reference) |

---

## 2. Core Language Invariants & Rules

1. **Mandatory Function Return Types**: Every `fn` must explicitly declare a return type (e.g. `: null`, `: i32`, `: [u8]`).
2. **Parameters are `const`**: Function parameters and `for` loop bindings are immutable. You cannot reassign them.
3. **No Semicolons Required**: Semicolons are optional and standard code omits them.
4. **Strict Scope & Shadowing**:
   - Same-block shadowing is **INVALID** (`var x = 1; var x = 2` fails).
   - Nested-block shadowing is allowed.
5. **Data Forms vs Behavior**:
   - Structs (`struct`), Enums (`enum`), and Tagged Unions (`choice`) store data shapes only.
   - Methods belong to anchor declarations: `fn TypeName::methodName(self, ...): ReturnType`.
6. **Built-in Array Surface**: Arrays have **only** the `.length` property. Mutation/resizing uses target functions or standard library modules. Negative indexing (e.g. `arr[-1]`) counts from the end; out-of-bounds returns `null`.
7. **Wildcard `_`**: Used in pattern matching, tuple destructuring, and default parameter placeholders (`func(1, _, 3)`). Cannot be read as a value (`var x = _` is invalid).

---

## 3. Async / Await Pattern Guide

Galfus Script has explicit, top-class support for asynchronous execution via `Future<T>` and `async`/`await`. 

> [!NOTE]
> In `fn(async)` function declarations, declare the inner payload return type (e.g. `: [u8]` or `: User`), **not** `Future<[u8]>`. The `fn(async)` metadata automatically wraps the return value in `Future<T>`.

```galfus
import { println } from "std/io"
import { Future } from "std/async"

// 1. Declare async function with fn(async)
fn(async) fetchUserData(userId: i64): [u8] {
  // 2. Single await
  const rawBytes = await loadBytesFromNetwork(userId)
  return rawBytes
}

fn(async) processConcurrently(idA: i64, idB: i64): i32 {
  // 3. Await All: runs futures concurrently and unwraps into a tuple
  const (dataA, dataB) = await(all) (
    fetchUserData(idA),
    fetchUserData(idB),
  )

  // 4. Await Race: resolves to whichever completes first (union type)
  const fastest = await(race) (
    fetchUserData(idA),
    fetchUserData(idB),
  )

  return 0
}
```

---

## 4. Module & Standard Library Guide

Imports and exports use explicit, unambiguous statements:

```galfus
import { print, println } from "std/io"
import { Future } from "std/async"
import { createThread } from "std/thread"
import math from "$math"

export struct User {
  id: i64,
  name: [u8],
}

export fn main(_args: [[u8]]): i32 {
  println("Hello Galfus")
  return 0
}
```

Standard Module Architecture:
1. **Internal Core Modules (Always Available, VM Native)**:
   - `std/async`: `Future<T>`
   - `std/thread`: `createThread`, `getThread`, `hasMessages`, `getMessage`
2. **Utility Modules (Always Available, Pure Galfus Script)**:
   - `text`: Byte array string utilities (`length`, `slice`, `toUpper`, `trim`)
   - `format`: `stringify<T>`, `parse<T>`
   - `json`: `parse<T>`, `stringify<T>`
   - `math`, `path`, `regex`, `collections`, `crypto`
3. **Bridge Modules (Optional, Paired with Host Providers)**:
   - `std/io`: `println`, `print`, `read` (uses `fn(async) __provider_io_*`)
   - `std/fs`, `std/net`, `std/process`, `std/time`, `std/gpio`
   - *Note*: Bridge modules use `fn(async) __provider_*` calls and exist only when registered in `galfus-workspace`.

---

## 5. Detailed Language Reference

For exhaustive documentation on lexical rules, generics, constraints, enums, choices, and VM semantics, refer to:
- [Language Reference](file:///.agents/skills/galfus-script/references/language_reference.md)
