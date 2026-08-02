Previous: [Data Forms](./07-data-forms.md) | Index: [Galfus Core System](./00-index.md) | Next: [Mutation, Assignment and Ownership](./09-mutation-assignment-and-ownership.md)

---

# 8. Expressions and Operators

This document defines expression forms, operator behavior, calls, member access, casts, assignment forms, and precedence.

## 8.1 Core Principle

Operators have fixed core meanings.

There is no operator overloading.

Custom behavior MUST use named functions.

```galfus
var result = a::add(b)
```

## 8.2 Literals and Names

Literal expressions:

```txt
integer
float
bool
null
string as [u8]
array literal
```

Name expressions read visible symbols.

```galfus
var next = count + 1
```

`_` is not readable.

## 8.3 Paths

Path expressions use `::`.

```galfus
math::max(10, 20)
Direction::North
Result::Ok(10)
user::rename("Ana")
```

The semantic resolver determines whether a path is a module function, enum variant, choice constructor, anchor call, or facade call.

## 8.4 Grouped, Tuple, and Array Expressions

Grouped expression:

```galfus
(1 + 2) * 3
```

Tuple expression:

```galfus
(10.0, 20.0)
```

Array literal:

```galfus
[1, 2, 3]
```

Empty arrays require expected type.

## 8.5 Construction Expressions

Struct construction:

```galfus
new(User) {
  id: 1,
  name: "Ana",
}
```

Array construction:

```galfus
new([i32], 10)
```

Inferred struct construction requires expected type.

```galfus
var user: User = new {
  id: 1,
  name: "Ana",
}
```

## 8.6 Copy Expression

`copy` performs explicit deep copy.

```galfus
var cloned = copy user
```

Assignment never deep-copies complex values.

## 8.7 Calls

Function call:

```galfus
sum(1, 2)
```

Path call:

```galfus
math::max(10, 20)
```

Anchor call:

```galfus
user::rename("Ana")
```

Anchor calls do not perform implicit write-back.

```galfus
user = user::rename("Ana")
```

## 8.8 Member Access

Field access:

```galfus
user.name
```

Null-safe field access:

```galfus
user.parent?.name
```

If the left side is `null`, the result is `null`.

Arrays expose only:

```galfus
values.length
```

## 8.9 Index Access

```galfus
values[0]
values[-1]
```

Negative indexes count from the end.

Out-of-bounds read returns `null`.

## 8.10 Casts

Explicit cast syntax:

```galfus
<i8> value
<bool> count
```

Casts must be validated by type rules.

## 8.11 Await Expressions

`await` expressions suspend execution until an asynchronous operation or `Future<T>` resolves.

### Single Await

Awaits a single asynchronous expression, unwrapping `Future<T>` into type `T`.

```galfus
const user = await loadUser(id)
```

### Concurrent Await All (`await(all)`)

Awaits a tuple of asynchronous expressions concurrently. Resolves to a tuple of unwrapped values `(T1, T2, ...)` when all futures complete.

```galfus
const result = await(all) (
  loadUser(id),
  loadPermissions(id),
)
```

### Concurrent Await Race (`await(race)`)

Awaits a tuple of asynchronous expressions concurrently and resolves to the value of whichever completes first. Returns a union type `T1 | T2 | ...`.

```galfus
const winner = await(race) (
  fetchFromPrimary(),
  fetchFromBackup(),
)
```

## 8.12 Unary Operators

```txt
!
~
-
copy
```

Meanings:

```txt
! -> boolean negation
~ -> bitwise not
- -> numeric negation
copy -> deep copy expression
```

## 8.13 Binary Operators

Arithmetic:

```txt
+ - * / % **
```

Comparison:

```txt
== != < <= > >=
```

Boolean:

```txt
&& ||
```

Bitwise:

```txt
& | ^ ~ << >>
```

`+` is not string concatenation.

## 8.14 Null Fallback

```galfus
var value = maybeValue ?? fallback
```

Meaning:

```txt
if maybeValue is not null -> maybeValue
else -> fallback
```

## 8.15 Assignment Operators

```txt
=
+= -= *= /= %= **=
&= |= ^= <<= >>= ??=
```

Assignment is statement-only.

Invalid:

```galfus
var value = count = 10
```

Fallback assignment:

```galfus
cache ??= createCache()
```

The target must be writable and nullable.

## 8.16 Operator Precedence

Recommended precedence from highest to lowest:

```txt
1. primary: literals, names, grouped, arrays, tuples, new
2. postfix/access: call, index, ., ?., ::
3. unary: !, ~, -, copy, explicit cast, await
4. exponent: **
5. multiplicative: *, /, %
6. additive: +, -
7. shift: <<, >>
8. comparison: <, <=, >, >=
9. equality: ==, !=
10. bitwise AND: &
11. bitwise XOR: ^
12. bitwise OR: |
13. boolean AND: &&
14. boolean OR: ||
15. null fallback: ??
16. assignment
```

Comparison and equality chaining SHOULD be rejected.

Invalid:

```galfus
a < b < c
```

Valid:

```galfus
a < b && b < c
```

## 8.17 Contract

The parser/checker MUST:

- Reject operator overloading.
- Reject string concatenation through `+`.
- Reject assignment in value-required positions.
- Type null-safe access as nullable.
- Support `copy` as a unary expression form.
- Support `await`, `await(all)`, and `await(race)` expressions.
- Keep `values.length` as the only built-in array property.
- Reject invalid assignment targets.

---

Previous: [Data Forms](./07-data-forms.md) | Index: [Galfus Core System](./00-index.md) | Next: [Mutation, Assignment and Ownership](./09-mutation-assignment-and-ownership.md)
