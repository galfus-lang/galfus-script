Previous: [Source, Names and Modules](./02-source-names-and-modules.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Type System, Inference and Propagation](./04-type-system-inference-and-propagation.md)

---

# 3. Bindings, Literals and Primitive Types

This document defines `var`, `const`, binding defaults, literal typing, primitive values, strings, and shadowing.

## 3.1 Bindings

`var` creates a mutable binding.

```galfus
var count = 10
count = 20
```

`const` creates an immutable binding.

```galfus
const count = 10
```

Invalid:

```galfus
const count = 10
count = 20
```

Function parameters and `for` item/index bindings are constant bindings by default.

## 3.2 Binding Initialization

A binding with an initializer infers or checks its type.

```galfus
var count = 10
var total: i64 = 10
```

A binding without initializer is valid only with an explicit type and a valid default.

Valid:

```galfus
var count: i32
var enabled: bool
var user: User | null
```

Defaults:

```txt
bool -> false
integer -> 0
float -> 0.0
nullable -> null
null -> null
```

Invalid:

```galfus
var count
const enabled
var user: User
```

A non-primitive non-nullable type has no default unless the language later defines explicit default construction.

## 3.3 Primitive Types

Core primitive scalar types:

```txt
bool
i8 i16 i32 i64 i128
u8 u16 u32 u64 u128
f32 f64
```

`null` is a literal and type used for nullable values.

There is no `undefined`.

## 3.4 Integer Literals

Integer literals may be written as:

```txt
decimal
binary 0b...
octal 0o...
hex 0x...
```

Examples:

```galfus
10
0b1010
0o12
0x0A
```

Default integer literal type is `i32` unless an expected type refines it.

```galfus
var value: i64 = 10
```

The literal is checked against the expected type.

## 3.5 Float Literals

Float literals default to `f32` unless an expected type refines them.

```galfus
var value = 10.5
var precise: f64 = 10.5
```

`float128` is not a core primitive type.

## 3.6 Negative Numbers

A negative number is parsed as unary minus over a positive literal.

```galfus
-10
```

Conceptual syntax:

```txt
-(10)
```

## 3.7 Numeric Separators

Numeric separators SHOULD use `_` between digits.

Valid:

```galfus
1_000_000
0xff_ff
```

Invalid forms SHOULD be rejected:

```txt
_100
100_
1__000
```

## 3.8 Boolean and Null

Boolean literals:

```galfus
true
false
```

`null` is valid only where the target type accepts `null`.

```galfus
var user: User | null = null
```

Invalid:

```galfus
var user: User = null
```

## 3.9 String Literals

A string literal is a UTF-8 byte array.

```galfus
var name = "Ana"
```

Type:

```txt
[u8]
```

There is no core `String` object.

Text behavior belongs to explicit modules.

## 3.10 Array Literals

Array literals infer an element type or use an expected type.

```galfus
var values = [1, 2, 3]
var wide: [i64] = [1, 2, 3]
```

Empty array literals require expected type.

Invalid:

```galfus
var values = []
```

Valid:

```galfus
var values: [i32] = []
```

## 3.11 Boolean Contexts

Boolean contexts use deterministic cast-to-bool behavior when the expression is not already `bool`.

Conceptual examples:

```txt
null -> false
false -> false
true -> true
0 -> false
nonzero -> true
```

Aggregate-to-bool SHOULD be rejected unless a later section explicitly defines it.

## 3.12 Shadowing

Same-block shadowing is invalid.

```galfus
var value = 10
var value = 20
```

Nested-block shadowing is valid.

```galfus
var value = 10

if ready {
  var value = 20
}
```

Top-level duplicates are invalid.

Import shadowing is invalid in module scope.

## 3.13 Contract

The checker MUST:

- Distinguish `var` and `const` binding mutability.
- Require explicit type for uninitialized bindings.
- Allow default initialization only for supported defaultable types.
- Reject plain `var value = null` when no expected nullable type exists.
- Type string literals as `[u8]`.
- Reject `undefined`.
- Reject same-block shadowing.
- Treat function parameters and `for` item/index bindings as constant bindings.

---

Previous: [Source, Names and Modules](./02-source-names-and-modules.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Type System, Inference and Propagation](./04-type-system-inference-and-propagation.md)
