# Galfus Script Complete Language Reference

This document serves as the authoritative syntax and semantic reference for **Galfus Script** (`.gfs`).

---

## 1. Syntax & Lexical Rules

- **Case-Sensitive**: Identifiers are case-sensitive (`userName` != `UserName`).
- **Naming Conventions**:
  - `PascalCase`: Types, structs, enums, choices, constraints.
  - `camelCase`: Functions, variables, fields, import bindings.
- **Statements**: Semicolons are optional and generally omitted.
- **Comments**: Single-line `//` and block `/* ... */`.

### 1.1 Reserved Words

```txt
import   export   var      const     type     struct
enum     choice   constraint satisfies fn       self
new      copy     if       else      match    instanceof
typeof   for      in       loop      break    continue
return   true     false    null      weak     await
```

### 1.2 Non-Existent / Forbidden Keywords

These keywords do **NOT** exist in Galfus Script:

```txt
while    do       try      catch     throw    commit
String   string   str      void      any      unknown
```

- **Loops**: There is no `while` loop. Use `loop`, `loop(name: id)`, `loop(after)`, or `for item in sequence`.
- **Error Handling**: No `try/catch/throw`. Errors are returned as values (e.g. `choice Result<V, E> { Ok(V), Err(E) }`).
- **Strings**: Strings are `[u8]` (UTF-8 byte arrays). There is no built-in `String` object.

### 1.3 Wildcard `_`

`_` is a non-readable wildcard placeholder:

- Used in pattern matching (`match` arms, `instanceof` arms).
- Used in tuple & array destructuring.
- Used in function calls as default-argument placeholders: `connect("localhost", _, 8080)`.
- Cannot be assigned as a variable (`var _ = expr` is invalid).
- Cannot be read as an expression (`var x = _` is invalid).

---

## 2. Type System & Variables

### 2.1 Mutability

- `var`: Creates a mutable binding (`var count = 10; count = 20`).
- `const`: Creates an immutable binding (`const name = "Ana"`).
- Function parameters and `for` loop bindings are `const` by default.

### 2.2 Variable Declarations & Default Initialization

- Initialized: `var count = 10` (infers type `i32`) or `var total: i64 = 10`.
- Uninitialized bindings MUST specify an explicit type: `var total: i64`.
- Default values for uninitialized typed primitive variables:
  - `bool`: `false`
  - `i8`..`i128`, `u8`..`u128`: `0`
  - `f32`, `f64`: `0.0`
  - Nullable types `T | null`: `null`
- Non-primitive, non-nullable types cannot be left uninitialized without an explicit default.

### 2.3 Shadowing Rules

- **Same-block shadowing is INVALID**: `var x = 1; var x = 2; // Error`
- **Nested-block shadowing is VALID**:
  ```galfus
  var x = 10
  if condition {
    var x = 20 // Valid inside block
  }
  ```

### 2.4 Primitive Types

```txt
bool
i8  i16  i32  i64  i128
u8  u16  u32  u64  u128
f32 f64
null
```

### 2.5 Union Types & Type Narrowing

- Union types: `type Scalar = i32 | bool | null`.
- `instanceof` narrows dynamic union values:
  ```galfus
  const val: Scalar = 10
  const num = instanceof val {
    i32 n => n,
    bool b => 1,
    null => 0,
  }
  ```
- `typeof` dispatches over generic type parameters:
  ```galfus
  fn checkType<T: Scalar>(): i32 {
    return typeof T {
      i32 => 32,
      bool => 1,
      null => 0,
      _ => -1,
    }
  }
  ```

---

## 3. Data Forms

### 3.1 Arrays (`[T]`)

- Typed arrays: `[i32]`, `[u8]`.
- Array creation with `new`: `var arr = new([i32], 10)` (creates array of 10 zeros).
- Array literals: `const list = [1, 2, 3]`. Spread: `[...first, 4, 5]`.
- Indexing: `list[0]`, negative index `list[-1]`. Out-of-bounds returns `null`.
- Built-in properties: **Only** `list.length`. (No `.push()`, `.pop()`, `.map()`).

### 3.2 Structs

- Definition:
  ```galfus
  struct User {
    id: i64,
    name: [u8],
    age: i32 = 0,               // Default field value
    const createdAt: i64,       // Const field (immutable after creation)
    weak parent: User | null,   // Weak reference (must be nullable)
  }
  ```
- Instantiation:
  ```galfus
  const user = new(User) {
    id: 1,
    name: "Ana",
    createdAt: 1000,
  }
  ```
- Inferred instantiation (when expected type is present):
  ```galfus
  const u: User = new { id: 1, name: "Ana", createdAt: 1000 }
  ```
- Struct Spread: `new(User) { ...user, name: "Bia" }`.
- Struct Expansion in declaration: `struct Employee { ...User, role: [u8] }`.

### 3.3 Enums & Choices

- **Enum**: Integer discriminant values:

  ```galfus
  enum Direction { North, South, East, West }
  enum(u8) Priority { Low(1), Medium(2), High(3) }
  ```

  Access via `Direction::North`. Cast to int: `<i32> Direction::North`.

- **Choice**: Tagged unions (sum types with payloads):
  ```galfus
  choice Outcome<T> {
    Ok(T),
    Err([u8]),
  }
  ```
  Matching choices:
  ```galfus
  const val = match outcome {
    Outcome::Ok(data) => data,
    Outcome::Err(_) => 0,
  }
  ```

### 3.4 Identity vs Copy

- Assignment of complex types (structs, arrays) preserves identity / graph reference.
- Explicit deep copy uses `copy`: `var cloned = copy user`.
- Fieldless structs (e.g. `struct Identity {}`) cannot be copied with `copy`.

---

## 4. Functions & Calls

### 4.1 Declaration & Return Types

- **Every function MUST specify a return type**:
  ```galfus
  fn add(a: i32, b: i32): i32 {
    return a + b
  }
  fn log(msg: [u8]): null {
    println(msg)
  }
  ```
- Expression body short form: `fn double(n: i32): i32 => n * 2`.
- Block bodies require explicit `return` on every non-null path.

### 4.2 Anchor Functions (Methods)

- Methods attached to types use the anchor syntax `fn Type::method(self, ...)`:
  ```galfus
  fn Point::move(self, dx: i32, dy: i32): Point {
    return new { x: self.x + dx, y: self.y + dy }
  }
  ```
- Method invocation uses `::`: `point::move(3, 4)`. (Field access uses `.`: `point.x`).

### 4.3 Function Expressions / Lambdas

```galfus
const double = fn(value: i32): i32 => value * 2
const sum = fn(a: i32, b: i32): i32 {
  return a + b
}
```

---

## 5. Async & Await

### 5.1 Async Functions `fn(async)`

Asynchronous functions are declared with the `async` keyword metadata:

```galfus
fn(async) fetchUser(id: i64): User {
  const data = await loadUserData(id)
  return parseUser(data)
}
```

Anonymous function expressions:

```galfus
const load = fn(async) (id: i64): User => await loadUserData(id)
```

Async functions return `Future<T>` (from `std/async`).

> **Note**: The return type annotation of an `fn(async)` function specifies the inner payload type (e.g. `: User` or `: [u8]`), **not** `Future<User>`. The compiler automatically wraps the return value in `Future<T>` at runtime/lowering.

### 5.2 Await Expressions

- **Single Await**: `await expr`
  Unwraps `Future<T>` into `T`.

  ```galfus
  const user = await fetchUser(1)
  ```

- **Concurrent Await All**: `await(all) ( expr1, expr2, ... )`
  Awaits all futures concurrently, returning a tuple of unwrapped values `(T1, T2, ...)`.

  ```galfus
  const (user, permissions) = await(all) (
    fetchUser(id),
    fetchPermissions(id),
  )
  ```

- **Concurrent Await Race**: `await(race) ( expr1, expr2, ... )`
  Awaits futures concurrently and returns the value of whichever resolves first (returns union `T1 | T2 | ...`).
  ```galfus
  const result = await(race) (
    fetchPrimary(),
    fetchBackup(),
  )
  ```

---

## 6. Control Flow & Iteration

### 6.1 Branching

```galfus
if condition {
  // ...
} else if otherCondition {
  // ...
} else {
  // ...
}
```

### 6.2 Loops

- **Infinite Loop**:
  ```galfus
  loop {
    if condition { break }
  }
  ```
- **Named Loop**:
  ```galfus
  loop(name: outer) {
    loop {
      if condition { break outer }
    }
  }
  ```
- **Range Iteration**:
  - Exclusive range: `for i in 0..10`
  - Array iteration: `for item in items`
  - Quantity range: `for step in 2::4%2`

---

## 7. Constraints & Generics

### 7.1 Constraints (Traits)

```galfus
constraint Renderable {
  fn render(self): [u8]
}

struct Label satisfies Renderable {
  text: [u8],
}

fn Label::render(self): [u8] {
  return self.text
}
```

### 7.2 Generics

```galfus
struct Box<T: Scalar> {
  value: T,
}

fn identity<T>(value: T): T {
  return value
}
```

---

## 8. Modules & Standard Library

### 8.1 Imports & Exports

```galfus
import { println } from "std/io"
import { Future } from "std/async"
import math from "$math"

export struct User { id: i64 }
export fn main(args: [[u8]]): i32 { return 0 }
```

### 8.2 Standard Builtin Modules Architecture

1. **Internal Core Modules (VM Native, Always Included)**:
   - `std/async`: `Future<T>`
   - `std/thread`: `createThread(fn, key)`, `getThread(key)`, `hasMessages()`, `getMessage()` (uses `__internal_*` primitives)

2. **Utility Modules (Pure Galfus Script, Always Included)**:
   - `text`: Byte array string utilities (`length`, `slice`, `toUpper`, `trim`)
   - `format`: `stringify<T>(val)`, `parse<T>(s)`
   - `json`: `parse<T>(bytes)`, `stringify<T>(val)`
   - `math`, `path`, `regex`, `collections`, `crypto`

3. **Bridge Modules (Optional Host Capabilities)**:
   - `std/io`: `print(text)`, `println(text)`, `read(until)`
   - `std/fs`, `std/net`, `std/process`, `std/time`, `std/gpio`
   - _Note_: Bridge modules use explicit `fn(async) __provider_*` declarations and require host registration in `galfus-workspace`.
