Previous: [Generics](./05-generics.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Data Forms](./07-data-forms.md)

---

# 6. Constraints as Traits

This document defines Galfus constraints as Rust-like traits, generic bounds, facade types, and dispatch surfaces.

## 6.1 Core Principle

A constraint defines a required surface.

A type that satisfies the constraint must provide that surface.

Constraints may be used as:

```txt
generic bounds
facade types
compiler-known behavior contracts
```

## 6.2 Constraint Declaration

```galfus
constraint Drawable {
  fn draw(self): null
}
```

Field requirement:

```galfus
constraint Identifiable {
  id: i64,
}
```

Generic constraint:

```galfus
constraint Comparable<T> {
  fn compare(self, other: T): i32
}

`self` is inferred. It does not use an explicit type annotation.

Semantics of `compare(self, other: T): i32`:
- Returns a negative value if `self` is ordered before `other`.
- Returns zero if `self` is equivalent to `other` in ordering.
- Returns a positive value if `self` is ordered after `other`.
```

## 6.3 Satisfying Constraints

A struct may declare that it satisfies constraints.

```galfus
struct Button satisfies Drawable {
  label: [u8],
}

fn Button::draw(self): null {
  return
}
```

Multiple constraints:

```galfus
struct User satisfies Identifiable, Drawable {
  id: i64,
  name: [u8],
}
```

## 6.4 Explicit Imports

Builtin constraints are compiler-known but not globally imported.

If a programmer references them directly, they must import them.

```galfus
import { Comparable, Iterable, Iterator } from "std/constraints"
```

## 6.5 Constraints as Generic Bounds

```galfus
fn drawItem<T: Drawable>(item: T): null {
  item::draw()
  return
}
```

Inside the generic function, only the bound surface is available.

## 6.6 Constraints as Facade Types

A constraint may be used as a facade type.

```galfus
var item: Drawable = button
item::draw()
```

A facade exposes only the constraint surface.

Invalid:

```galfus
item.label
```

unless `label` is required by `Drawable`.

Field requirements are visible through facade values.

```galfus
var item: Identifiable = user
var id = item.id
```

## 6.7 Facade Is Not Concrete Type

A facade value is not the same as its concrete type.

```galfus
var drawable: Drawable = button
```

To recover a concrete type, use `instanceof`.

```galfus
var label = instanceof drawable {
  Button button => button.label,
  _ => "unknown",
}
```

## 6.8 Constraint Composition

Constraint composition may be used for bounds and facades.

```galfus
fn save<T: Identifiable + Serializable>(value: T): null {
}

var item: Identifiable + Drawable = button
```

Composition exposes the union of required surfaces.

## 6.9 No Operator Overloading

Constraints do not overload operators.

Invalid:

```galfus
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

## 6.10 Iterator and Iterable

Conceptual forms:

```galfus
constraint Iterator<Item> {
  fn next(self): Item | null
}

constraint Iterable<Item, Iter> {
  fn iter(self): Iter
}
```

The compiler knows the iteration concept, but source code must import these names to use them directly.

## 6.11 Constraints and `instanceof`

`instanceof` over open facade values usually requires `_`.

```galfus
var label = instanceof drawable {
  Button button => button.label,
  Icon icon => icon.name,
  _ => "drawable",
}
```

Without a closed known set, the checker cannot prove exhaustiveness.

## 6.12 Dispatch

Constraint dispatch may lower to:

```txt
static direct call when concrete type is known
dispatch metadata when facade value is dynamic
compact vtable-like representation
```

Observable behavior must remain deterministic.

## 6.13 Contract

The checker MUST:

- Validate all required fields and functions for `satisfies`.
- Reject missing or incompatible requirements.
- Keep facade surfaces limited to constraint requirements.
- Reject operator overloading through constraints.
- Require explicit import when constraint names are referenced directly.
- Validate constraint composition deterministically.

---

Previous: [Generics](./05-generics.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Data Forms](./07-data-forms.md)
