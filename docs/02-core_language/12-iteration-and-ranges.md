Previous: [Control Flow](./11-control-flow.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Pattern Matching and Narrowing](./13-pattern-matching-and-narrowing.md)

---

# 12. Iteration and Ranges

This document defines `for`, named `for`, item/index bindings, compiler-known iteration concepts, arrays in iteration, and range literals.

## 12.1 Basic `for`

```galfus
for value in values {
  log(value)
}
```

With index:

```galfus
for value, index in values {
  log(index)
  log(value)
}
```

The first binding receives the item.

The second receives the zero-based index.

Item and index bindings are constant bindings.

```galfus
for value, index in values {
  value = 10 // invalid
  index = 1  // invalid
}
```

## 12.2 Wildcard in `for`

Ignore item:

```galfus
for _, index in values {
  log(index)
}
```

Ignore index:

```galfus
for value, _ in values {
  log(value)
}
```

## 12.3 Named `for`

```galfus
for(name: users) user in users {
  loop {
    break users
  }
}
```

`break users` exits the named `for`.

`continue users` advances it to the next iteration.

## 12.4 Source Evaluation

The source expression is evaluated once.

```galfus
for value in createRange() {
  log(value)
}
```

Conceptual behavior:

```txt
source = createRange()
iterator = source::iter()
repeat next()
```

## 12.5 Compiler-Known Iteration Concepts

`Iterable` and `Iterator` are known by the compiler for validation and lowering. Range literals are compiler-known expression forms backed by `std/range`.

If source code references `Iterable` or `Iterator` directly, it must import them.

```galfus
import { Iterable, Iterator } from "std/constraints"
```

Compiler-known does not mean globally available.

## 12.6 Iterator and Iterable

Conceptual model:

```galfus
constraint Iterator<Item> {
  fn next(self): Item | null
}

constraint Iterable<Iter: Iterator> {
  fn iter(self): Iter
}
```

Iteration completes when `next()` returns `null`.

If nullable items are required, a future iterator shape may use a choice such as `Next<T>`.

## 12.7 Arrays in `for`

Arrays are iterable in index order.

```galfus
var values = [10, 20, 30]

for value, index in values {
  log(index)
  log(value)
}
```

Array iteration order:

```txt
0
1
2
...
length - 1
```

Mutating the iterated array structure during iteration SHOULD be rejected initially.

Mutating fields inside complex elements may be allowed if normal mutation rules allow it.

## 12.8 Range Literals

Supported literal range forms:

```txt
start..end
start::count
start::count%step
```

Examples:

```galfus
1..9
1::4
1::4%3
1.5::4%0.5
```

Range operands must be literals. Dynamic expressions are not range operands.

`start..end` accepts integer literals only and produces `RangeExclusive`.

`start::count` accepts an integer or float literal start, an integer literal count, and produces `RangeStepped<i64>` or `RangeStepped<f64>`.

`start::count%step` accepts an integer or float literal start, an integer literal count, and an integer or float literal step. If either `start` or `step` is a float literal, integer literals are promoted and the result is `RangeStepped<f64>`. Otherwise the result is `RangeStepped<i64>`.

Invalid:

```galfus
a..b
start::count
1.5..10.5
```

Dynamic ranges should be built through explicit functions/modules if needed later.

## 12.9 Exclusive Range `start..end`

`start..end` is exclusive.

```galfus
1..4
```

Produces:

```txt
1
2
3
```

The end value is not included.

The range must produce at least one value. In other words, `end - start` must not be zero.

Invalid:

```galfus
1..1
```

Descending ranges are valid if non-empty.

```galfus
4..1
```

Produces:

```txt
4
3
2
```

Exclusive range items use `i64` by default.

## 12.10 Quantity Range `start::count`

`start::count` produces `count` values.

```galfus
1::4
```

Produces:

```txt
1
2
3
4
```

`count` must be positive.

Invalid:

```galfus
1::0
1::-1
```

If start is an integer literal, items use `i64` by default. If start is a float literal, items use `f64` by default.

## 12.11 Quantity Range with Step

```galfus
1::4%3
```

Produces:

```txt
1
4
7
10
```

Step must not be zero.

Invalid:

```galfus
1::2%0
```

Negative step is valid.

```galfus
4::3%-1
```

Produces:

```txt
4
3
2
```

Step chooses the stepped range numeric family:

```galfus
1::4%2       // valid: RangeStepped<i64>
1.0::4%0.5  // valid: RangeStepped<f64>
1::4%0.5    // valid: RangeStepped<f64>
```

## 12.12 Range Allocation

Ranges do not materialize arrays.

They lower to lightweight iterable values or direct iteration state.

Two loops over the same range are independent.

## 12.13 Iterating Complex Values

Iteration does not deep-copy complex items.

```galfus
for user in users {
  user.name = "Updated"
}
```

The item binding is const, but the reachable graph may still be mutable if normal rules allow it.

Use `copy` for independent values.

## 12.14 Contract

The checker/lowering MUST:

- Evaluate `for` source once.
- Treat item and index bindings as const.
- Support named `for` targets.
- Keep `Iterable` and `Iterator` compiler-known but not globally imported.
- Require explicit imports when `Iterable` or `Iterator` is referenced directly in source.
- Restrict literal ranges to integer literals.
- Reject `1..1`, `1::0`, `1::-1`, and `1::2%0`.
- Avoid implicit array materialization for ranges.
- Avoid implicit deep copy of iteration items.

---

Previous: [Control Flow](./11-control-flow.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Pattern Matching and Narrowing](./13-pattern-matching-and-narrowing.md)
