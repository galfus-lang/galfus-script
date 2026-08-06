Previous: [Expressions and Operators](./08-expressions-and-operators.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Functions and Calls](./10-functions-and-calls.md)

---

# 9. Mutation, Assignment and Ownership

This document defines mutation, assignment, copy, anchors, edges, weak observers, temporaries, and release behavior.

## 9.1 Core Principle

Primitive scalar assignment copies the value.

Complex assignment shares the existing value graph.

Deep copy requires `copy`.

```galfus
var other = user       // shares graph
var cloned = copy user // deep copy
```

## 9.2 Primitive Assignment

Primitive scalar values are copied by value.

```galfus
var a = 10
var b = a
b = 20
```

`a` remains `10`.

## 9.3 Complex Assignment

Complex values include:

```txt
structs
arrays
tuples
choices with payloads
byte strings as [u8]
constraint facade values
adapter/resource values
```

Complex assignment shares the graph.

```galfus
var user = new(User) {
  name: "Ana",
}

var other = user
other.name = "Bia"
```

Both `user.name` and `other.name` observe `"Bia"`.

## 9.4 Binding Anchors

A binding may act as an ownership anchor.

```galfus
var user = new(User) {
  name: "Ana",
}
```

Conceptually:

```txt
user -> anchor -> User graph
```

Reassigning a binding removes the binding root from the old graph and attaches it to the new graph.

## 9.5 `var` and `const`

`var` may be reassigned.

```galfus
var count = 10
count = 20
```

`const` cannot be reassigned.

```galfus
const count = 10
count = 20 // invalid
```

A `const` binding does not automatically freeze the reachable graph.

```galfus
const user = new(User) {
  name: "Ana",
}

user.name = "Bia" // valid if field and graph rules allow it
```

## 9.6 Function and Iteration Bindings

Function parameters are constant bindings by default.

```galfus
fn call(value: i32): null {
  value = 10 // invalid
}
```

`for` item and index bindings are also constant.

```galfus
for value, index in values {
  value = 10 // invalid
  index = 1  // invalid
}
```

## 9.7 Field Assignment

Field assignment mutates a struct field.

```galfus
user.name = "Bia"
```

The field must be mutable and type-compatible.

`const` fields cannot be reassigned after construction.

## 9.8 Index Assignment

```galfus
values[0] = 10
```

The array value must be mutable, and the assigned value must match the element type.

Out-of-bounds read returns `null`.

Out-of-bounds write is a deterministic runtime error.

## 9.9 Assignment Statement

Assignment is not a value expression.

Valid:

```galfus
count = 10
```

Invalid:

```galfus
var value = count = 10
```

Invalid:

```galfus
if count = 10 {
}
```

## 9.10 Compound Assignment

```galfus
count += 1
```

Conceptual lowering:

```txt
count = count + 1
```

The operation must be valid and the result must be assignable to the target.

## 9.11 Fallback Assignment

```galfus
cache ??= createCache()
```

Meaning:

```txt
if cache is null:
  cache = createCache()
```

The target must be writable and nullable.

## 9.12 Copy

`copy` creates an explicit deep copy.

```galfus
var cloned = copy user
```

`copy` must:

```txt
duplicate owning topology
preserve ownership topology inside the copy
validate weak observers
respect resource copy policy
reject non-copyable values
```

## 9.13 Weak Fields and Copy

Weak fields are not copied blindly.

Rules:

```txt
weak target is null -> copied weak field is null
weak target is alive and inside copied topology -> copied weak points to copied target
weak target is alive but outside copied topology -> copied weak may remain weak if policy allows
weak target is not alive -> copied weak field is null
```

`copy` MUST NOT promote weak observers into owning edges.

## 9.14 Fieldless Struct Copy Rejection

Fieldless structs cannot be copied with `copy`.

```galfus
struct RuntimeToken {}

var token2 = copy token // invalid
```

They behave like opaque runtime identities, unique ids, or pointer-like handles.

Assignment still shares the same identity.

```galfus
var token2 = token
```

## 9.15 Ownership Graph

Ownership uses:

```txt
anchors
edges
weak observers
```

Fields, array elements, tuple elements, and choice payloads may create edges.

Weak fields create weak observers.

A value lives while reachable from at least one anchor through owning edges.

## 9.16 Release Points

Release happens at deterministic safe points.

Recommended safe points:

```txt
end of statement
after reassignment
end of block
function return
safe loop iteration boundary
```

Weak observers become `null` when their target is released.

## 9.17 Temporaries

Temporaries live until the end of the full expression unless anchored or stored through an owning edge.

```galfus
var name = createUser().name
```

The temporary user lives long enough for the field read.

## 9.18 Cycles

Owning edges may form cycles.

Cycles are valid while reachable from an anchor.

If a cycle becomes unreachable, the owner graph must release it deterministically.

## 9.20 Contract

The checker/lowering MUST:

- Copy primitive scalars by value.
- Share complex assignment graphs.
- Reject implicit deep copy.
- Support `copy` as explicit deep copy.
- Reject `copy` for fieldless structs.
- Validate weak field nullability.
- Validate weak behavior during copy.
- Reject invalid assignment targets.
- Reject assignment in value-required positions.
- Release unreachable graphs deterministically.

---

Previous: [Expressions and Operators](./08-expressions-and-operators.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Functions and Calls](./10-functions-and-calls.md)
