Previous: [Reserved Words and Lexical Rules](./01-reserved-words-and-lexical-rules.md) | Index: [Galfus Core System](./00-index.md) | Next: [Bindings, Literals and Primitive Types](./03-bindings-literals-and-primitive-types.md)

---

# 2. Source, Names and Modules

> **Status note:** `.gfs` source modules are implemented. References to `.gfp`
> proxy modules describe planned tooling and are not accepted by the current
> workspace loader.

This document defines source files, top-level declarations, imports, exports, module addresses, names, and visibility.

## 2.1 Source Files

A `.gfs` file is a Galfus source file.

A source file may contain top-level declarations:

```txt
import
export var
export const
export type
export struct
export enum
export choice
export constraint
export fn
export fn(metadata)
var
const
type
struct
enum
choice
constraint
fn
fn(metadata)
```

Top-level control-flow statements are invalid.

Invalid:

```galfus
if ready {
  run()
}
```

## 2.2 Comments

Line comments and block comments are supported.

```galfus
// line comment

/* block comment */
```

Recommended initial rule: block comments are not nested.

## 2.3 Names and Paths

Path access uses `::`.

```galfus
math::max(10, 20)
Result::Ok(10)
user::rename("Ana")
```

Field access uses `.`.

```galfus
user.name
values.length
```

Null-safe field access uses `?.`.

```galfus
user.parent?.name
```

## 2.4 Source Modules

Each resolved `.gfs` file is a source module.

Symbols are private by default.

```galfus
fn helper(): i32 {
  return 10
}
```

A symbol becomes public only with `export`.

```galfus
export fn value(): i32 {
  return helper()
}
```

## 2.5 Imports

Whole-module import:

```galfus
import user from "./user"

var created = user::createUser()
```

Named import:

```galfus
import { User, createUser } from "./user"

var created = createUser()
```

Import addresses may be:

```txt
relative path
alias path using $
dependency or builtin address
organization-qualified address using @
```

Examples:

```galfus
import math from "$math"
import text from "std/text"
import vector from "@vulppi/math-core/vector"
```

The `$` prefix exists only at import sites. It is not used in `galfus.toml` alias keys.

## 2.6 Exports

Only explicitly exported declarations are visible from other modules.

```galfus
export struct User {
  id: i64,
  name: [u8],
}

export fn createUser(id: i64, name: [u8]): User {
  return new(User) {
    id,
    name,
  }
}
```

There is no implicit public namespace from file names or folders.

## 2.7 No Special Source File Names

Galfus source resolution MUST NOT assign implicit behavior to names such as:

```txt
main.gfs
root.gfs
index.gfs
mod.gfs
```

Entry points and exports are defined by workspace/module configuration, not by magic filenames.

## 2.8 Duplicate Symbols

Duplicate top-level symbols in the same source module are invalid.

```galfus
fn value(): i32 {
  return 1
}

fn value(): i32 {
  return 2
}
```

Import binding conflicts are invalid in the same module scope.

```galfus
import { User } from "./user"

struct User {
  id: i64,
}
```

Standard module names are not keywords. They are categorized into three distinct tiers:

1. **Internal Core Modules** (`std/async`, `std/thread`): Always included by default; execute within the VM engine using `__internal_*` primitives.
2. **Utility Modules** (`text`, `format`, `json`, `math`, `path`, `regex`, `std/constraints`, `std/iterable`): Always included by default; pure Galfus Script algorithmic utilities without native bridge calls.
3. **Bridge Modules** (`std/io`, `std/net`, `std/fs`, `std/process`, `std/time`, etc.): Optional host capability modules declared in atomic pairs (`HostProvider` + `.gfs` bridge source) and registered via `galfus-workspace`. All provider bridge functions MUST explicitly be declared with `fn(async) __provider_*`.

They are resolved by import resolution, not by lexical reservation.

## 2.10 Public Artifact Model

The source-level public file types are:

```txt
.gfs -> Galfus source
.gfp -> Galfus proxy/adaptor definition
```


Compiled modules are represented in memory as `BytecodeModule` values within a
`BytecodeGraph`. Serialization and final target bundles are planned, not
implemented.

## 2.11 Contract

The resolver MUST:

- Treat each `.gfs` as a source module.
- Keep local symbols private unless exported.
- Reject duplicate top-level symbols.
- Reject import binding conflicts.
- Resolve imports deterministically.
- Avoid implicit public namespace behavior.
- Avoid special source filename semantics.
- Keep standard module names non-reserved.

---

Previous: [Reserved Words and Lexical Rules](./01-reserved-words-and-lexical-rules.md) | Index: [Galfus Core System](./00-index.md) | Next: [Bindings, Literals and Primitive Types](./03-bindings-literals-and-primitive-types.md)
