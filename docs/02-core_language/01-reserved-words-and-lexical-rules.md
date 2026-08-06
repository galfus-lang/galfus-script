Previous: [Galfus Core System](./00-intro.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Source, Names and Modules](./02-source-names-and-modules.md)

---

# 1. Reserved Words and Lexical Rules

This document defines lexical rules, reserved words, identifiers, wildcard behavior, primitive names, and keyword metadata names.

## 1.1 Identifiers

Identifiers are case-sensitive.

```galfus
var userName = "Ana"
var UserName = "Bia"
```

`userName` and `UserName` are different identifiers.

Recommended style:

| Kind                                      | Style                                       |
| ----------------------------------------- | ------------------------------------------- |
| Type, struct, enum, choice, constraint    | `PascalCase`                                |
| Function, variable, field, import binding | `camelCase`                                 |
| Generic parameter                         | `PascalCase` or short uppercase such as `T` |
| Rust implementation internals             | `snake_case`                                |

## 1.2 Reserved Words

The following words are reserved by the core language:

```txt
import
export
var
const
type
struct
enum
choice
constraint
satisfies
fn
self
new
copy
if
else
match
instanceof
typeof
for
in
loop
break
continue
return
true
false
null
weak
await
```

The following words are intentionally not part of the language:

```txt
while
do
try
catch
throw
commit
```



## 1.3 Primitive Type Names

Core primitive names:

```txt
bool
null
i8
i16
i32
i64
i128
u8
u16
u32
u64
u128
f32
f64
```

These names are reserved as built-in type names.

Not core primitive names:

```txt
float128
String
string
str
byte
char
short
long
double
void
any
unknown
```

A string literal has type `[u8]`, not `String`.

`float128` is not core because Galfus avoids depending on unstable or target-fragile floating-point support for the base language.

## 1.4 Metadata Names Are Not Reserved

Keyword metadata names are not globally reserved.

The following names may be ordinary identifiers outside valid metadata positions:

```txt
stamp
after
name
async
```

Valid ordinary identifiers:

```galfus
var stamp = 10
var shared = true
var after = "done"
var name = "root"
var async = true
```

Valid keyword metadata:

```galfus
fn(stamp) call(): null {
}

fn(async) fetchData(): [u8] {
}

loop(after) ready {
}

loop(name: root) {
}
```

The resolver decides meaning from syntactic position.

## 1.5 `self`

`self` is reserved.

Anchor functions use `self` without an explicit type annotation.

Valid:

```galfus
fn User::rename(self, name: [u8]): User {
  self.name = name
  return self
}
```

Invalid:

```galfus
fn User::rename(self: User, name: [u8]): User {
  return self
}
```

The type of `self` is inferred from the anchor target.

## 1.6 Wildcard `_`

`_` is a wildcard token. It does not create a readable value.

Valid uses:

```txt
tuple destructuring
array destructuring
for item/index binding
match arms
instanceof arms
function call default placeholders
```

Invalid as a normal binding:

```galfus
var _ = compute()
```

Invalid as a readable expression:

```galfus
var value = _
```

Struct destructuring does not use `_` for omitted fields. Omit fields instead.

Invalid as an `instanceof` subject:

```galfus
instanceof _ {
  _ => 0
}
```

`instanceof` requires a real input expression.

## 1.7 Wildcard as Default-Argument Placeholder

`_` may request a default argument in a function call.

```galfus
fn call(a: i32, b: i32 = 2, c: i32 = 3): null {
}

call(1, _, 3)
```

Meaning:

```txt
a = 1
b = default
c = 3
```

`_` is valid only when the corresponding parameter has a default.

Invalid:

```galfus
fn call(a: i32, b: i32): null {
}

call(1, _)
```

Trailing default parameters may be omitted without `_`.

```galfus
call(1)
call(1, _)
call(1, _, _)
```

All are valid when trailing parameters have defaults. Empty argument gaps such as `call(1,,3)` are invalid. All omitted default arguments in non-trailing positions must use the `_` placeholder.

## 1.8 Operators Are Not Identifiers

Operators have fixed meanings and cannot be overloaded.

Operators are not normal identifiers.

A constraint cannot redefine `+`, `==`, `<`, or any other operator.

Custom behavior MUST use named functions.

```galfus
fn NumberBox::add(self, other: NumberBox): NumberBox {
  return new(NumberBox) {
    value: self.value + other.value,
  }
}
```

## 1.9 Contract

The lexer and parser MUST:

- Preserve case-sensitive identifiers.
- Reject reserved words in normal identifier positions.
- Treat metadata names as ordinary identifiers outside metadata positions.
- Treat `_` as wildcard only in supported positions.
- Reject `_` as a readable expression.
- Recognize `copy` as a reserved expression keyword.

---

Previous: [Galfus Core System](./00-intro.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Source, Names and Modules](./02-source-names-and-modules.md)
