Previous: [Functions and Calls](./10-functions-and-calls.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Iteration and Ranges](./12-iteration-and-ranges.md)

---

# 11. Control Flow

This document defines `if`, `else`, `loop`, named loops, `break`, `continue`, `return`, and reachability.

## 11.1 Branching

```galfus
if ready {
  run()
} else {
  stop()
}
```

`else if` chains are supported.

```galfus
if value < 0 {
  return -1
} else if value > 0 {
  return 1
} else {
  return 0
}
```

`if` is statement-only.

Invalid:

```galfus
var value = if ready {
  10
} else {
  20
}
```

Use `match` or `instanceof` for value-producing branching.

## 11.2 Blocks

Blocks create local scopes.

Nested shadowing is allowed.

Same-block shadowing is invalid.

Bindings are released at block end unless their values escape through a valid ownership path.

## 11.3 Loops

`loop` is the only general repetition construct.

Infinite loop:

```galfus
loop {
  run()
}
```

Condition-before loop:

```galfus
loop ready {
  run()
}
```

Condition-after loop:

```galfus
loop(after) ready {
  run()
}
```

There is no `while` or `do` keyword.

## 11.4 Named Loops

A loop may receive a name through keyword metadata.

```galfus
loop(name: root) {
  loop {
    break root
  }
}
```

The name is a control-flow label, not a value binding.

Invalid:

```galfus
loop(name: root) {
  var value = root
}
```

## 11.5 Named `for`

`for` may also receive a name.

```galfus
for(name: users) user in users {
  loop {
    break users
  }
}
```

`continue users` advances the named `for` to its next iteration.

## 11.6 `break`

Without a name, `break` exits the nearest loop or `for`.

```galfus
loop {
  if done {
    break
  }
}
```

With a name, it exits the named visible loop or `for`.

```galfus
break root
```

Invalid outside loops/for.

## 11.7 `continue`

Without a name, `continue` advances the nearest loop or `for`.

With a name, it advances the named visible loop or `for`.

```galfus
continue root
```

For `loop(after)`, `continue` jumps to the after-condition check.

## 11.8 Return

`return` exits the current function.

```galfus
return
return value
```

Bare `return` is valid only when the function return type accepts `null`.

Functions returning `null` may omit final return.

Non-null functions must return on all reachable paths.

## 11.9 Reachability

Unreachable code is a warning.

```galfus
fn value(): i32 {
  return 10

  var unused = 20
}
```

The statement after `return` is unreachable but compilation may continue.

## 11.11 Contract

The checker MUST:

- Reject `while` and `do`.
- Treat `if` as statement-only.
- Support `loop`, condition-before `loop`, and `loop(after)`.
- Support named `loop` and named `for` targets.
- Reject duplicate visible loop names.
- Reject invalid `break`/`continue` target names.
- Treat unreachable code as a warning.
- Require definite return for non-null functions.

---

Previous: [Functions and Calls](./10-functions-and-calls.md) | Index: [Galfus Core System](./00-intro.md) | Next: [Iteration and Ranges](./12-iteration-and-ranges.md)
