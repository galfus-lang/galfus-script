# Galfus Workspace Reference

> **Status: Historical design.** This document preserves a broader workspace,
> bundle, adapter, and distribution proposal. Those facilities are not part of
> the implemented architecture. For the current contract, see
> [Galfus Architecture Reference](../01-introduction/02-architecture_overview.md) and
> [Galfus Pipeline Boundaries](./02-pipeline_boundaries.md).

This document defines the Galfus workspace and project model.

The workspace model is explicit, deterministic, module-first, and designed to keep final bundles small. It describes how projects are organized on disk, how `galfus.toml` declares module identity and resolution rules, how dependencies are addressed, how artifacts are produced, and how final distribution units are assembled.

This document covers workspace structure only. Syntax, semantic rules, and runtime architecture are defined in separate documents.

---

## Table of Contents

1. [Design Goals](#1-design-goals)
2. [Module-First Project Model](#2-module-first-project-model)
3. [Project Root](#3-project-root)
4. [Standard Project Layout](#4-standard-project-layout)
5. [Root Directories](#5-root-directories)
6. [`galfus.toml`](#6-galfustoml)
7. [`[module]`](#7-module)
8. [Version Format](#8-version-format)
9. [Targets](#9-targets)
10. [Entrypoints](#10-entrypoints)
11. [`[exports]`](#11-exports)
12. [`[alias]`](#12-alias)
13. [`[dependencies]`](#13-dependencies)
14. [Import Address Model](#14-import-address-model)
15. [Valid Module Descriptors](#15-valid-module-descriptors)
16. [Artifact Roles](#16-artifact-roles)
17. [Galfus Proxy Files](#17-galfus-proxy-files)
18. [Adapter Payloads](#18-external-payloads)
19. [Galfus Map Files](#19-internal-debug-mapping)
20. [Workspace / Monorepo Layout](#20-workspace--monorepo-layout)
21. [Workspace Members](#21-workspace-members)
22. [Deterministic Resolution](#22-deterministic-resolution)
23. [Case Sensitivity](#23-case-sensitivity)
24. [Module Identity](#24-module-identity)
25. [Builtins and Standard Modules](#25-builtins-and-standard-modules)
26. [Compiler Module](#26-compiler-module)
27. [Build Behavior](#27-build-behavior)
28. [Debug and Release Builds](#28-debug-and-release-builds)
29. [Bundle Behavior](#29-bundle-behavior)
30. [Single Distribution Unit](#30-single-distribution-unit)
31. [Reachability and Tree-Shaking](#31-reachability-and-tree-shaking)
32. [Sandbox Configuration](#32-sandbox-configuration)
33. [Runtime Profiles](#33-runtime-profiles)
34. [Adapter Policy](#34-adapter-policy)
35. [Target Toolchains](#35-target-toolchains)
36. [Runtime Compilation Workspaces](#36-runtime-compilation-workspaces)
37. [Build Cache](#37-build-cache)
38. [`galfus.lock`](#38-galfuslock)
39. [Generated Files Policy](#39-generated-files-policy)
40. [IDE and Source Reconstruction Data](#40-ide-and-source-reconstruction-data)
41. [Publishing Model](#41-publishing-model)
42. [Dependency Direction](#42-dependency-direction)
43. [Validation Rules](#43-validation-rules)
44. [Non-Goals](#44-non-goals)
45. [Summary](#45-summary)

---

## 1. Design Goals

The Galfus workspace model is designed to be:

```txt
explicit
module-first
deterministic
small by default
friendly to static analysis
friendly to LLM inspection
safe for published artifacts
clear across desktop, server, web, mobile, and embedded targets
```

A Galfus workspace avoids implicit loader behavior. Imports do not behave like Node.js, Deno, Bun, or general-purpose bundler loaders. Imports resolve only to Galfus module descriptors.

---

## 2. Module-First Project Model

Galfus uses a module-first model.

A project is a graph of Galfus modules. A workspace is a larger graph that can group multiple module projects.

The term `package` is intentionally avoided in the project model. Everything becomes part of a module graph.

---

## 3. Project Root

A Galfus project root is identified by a `galfus.toml` file.

```txt
my-module/
  galfus.toml
```

The `galfus.toml` file defines the module identity, target kind, entrypoint, exports, aliases, dependencies, and target configuration.

---

## 4. Standard Project Layout

A standard project uses this layout:

```txt
my-module/
  galfus.toml
  src/
  modules/
  build/
```

A simple application:

```txt
my-app/
  galfus.toml
  src/
    main.gfs
  modules/
  build/
```

A library:

```txt
my-lib/
  galfus.toml
  src/
    vectors.gfs
    math.gfs
  modules/
  build/
```

A project with local binary/proxy dependencies:

```txt
my-app/
  galfus.toml
  src/
    main.gfs
  modules/
    physics.gfp
    libphysics.so
    audio.gfp
    libaudio.so
  build/
```

No filename is special by convention. There is no implicit `main.gfs`, `index.gfs`, `mod.gfs`, or `root.gfs`.

---

## 5. Root Directories

### `src/`

`src/` contains project-owned Galfus source files.

```txt
src/
  main.gfs
  user.gfs
  math/
    vector.gfs
```

Files inside `src/` become part of the graph only when referenced by configuration, import resolution, or another resolved module.

### `modules/`

`modules/` contains local Galfus module artifacts, proxy descriptors, and related external payloads.

It may contain:

```txt
.gfs
.gfp
.wasm
.dll
.so
.dylib
```

Only these are Galfus module descriptors:

```txt
.gfs
.gfp
```

External payloads such as `.wasm`, `.dll`, `.so`, and `.dylib` are not import targets. They are attached through `.gfp` files.

### `build/`

`build/` contains generated files and can be deleted.

It may contain:

```txt
compiler cache
module graph cache
MIR cache
bytecode cache
bundle outputs
diagnostics
hashes
logs
```

Deleting `build/` must not change the final result of a valid project, assuming all source files, descriptors, dependencies, payloads, and lock data remain available.

---

## 6. `galfus.toml`

`galfus.toml` is the explicit project configuration file.

A typical application configuration:

```toml
[module]
name = "my-app"
version = "0.0.1"
target = "app"
entry = "src/main.gfs"

[alias]
math = "src/math.gfs"

[dependencies]
collection = { target = "collection" }
physics = { target = "./modules/physics.gfp" }
audio = { target = "./modules/audio.gfp" }
```

A typical library configuration:

```toml
[module]
name = "math-core"
organization = "vulppi"
version = "1.0.0"
target = "lib"

[exports]
"vector" = "src/vector.gfs"
"matrix" = "src/matrix.gfs"
```

All behavior that affects entrypoints, public exports, aliases, dependencies, and targets must be explicit.

---

## 7. `[module]`

The `[module]` table defines module identity and target kind.

```toml
[module]
name = "my-module"
version = "0.0.1"
target = "app"
```

Supported fields:

```txt
name
organization
version
target
entry
```

### `name`

The module name.

```toml
name = "my-module"
```

### `organization`

Optional organization scope.

```toml
organization = "vulppi"
```

When present, public imports may use an organization-qualified address.

```galfus
import vectors from "@vulppi/math-core/vector"
```

### `version`

The module version. The version format is defined below.

### `target`

The target kind of the project module.

Initial target kinds:

```txt
app
lib
```

Additional platform output settings are defined by target configuration sections.

### `entry`

The entry source/module descriptor for an application, or for a library that wants a single entry surface.

```toml
entry = "src/main.gfs"
```

---

## 8. Version Format

Galfus module versions use this strict format:

```regex
/^(?<major>\d+)\.(?<minor>\d+)\.(?<patch>\d+)(?:-(?<tag>[a-z]+)\.(?<tagVersion>\d+))?$/
```

Valid examples:

```txt
0.0.1
1.2.3
1.0.0-alpha.1
2.4.8-beta.3
```

Invalid examples:

```txt
1
1.0
1.0.0-alpha
1.0.0-alpha.beta
1.0.0-rc
1.0.0-RC.1
```

The pre-release tag must be lowercase alphabetic text followed by a numeric tag version.

---

## 9. Targets

The `[module].target` field defines whether the module is an application or a library.

```toml
target = "app"
```

```toml
target = "lib"
```

Platform output configuration is separate. For example:

```toml
[target]
platform = "desktop"
output = "executable"
profile = "std"
```

Target configuration can describe output form and toolchain requirements, but it does not decide which modules are included. Modules are included by reachability and policy.

---

## 10. Entrypoints

Application modules require an explicit entrypoint.

```toml
[module]
target = "app"
entry = "src/main.gfs"
```

Library modules require either an explicit entrypoint or at least one explicit export.

```toml
[module]
target = "lib"
entry = "src/root.gfs"
```

or:

```toml
[module]
target = "lib"

[exports]
"vector" = "src/vector.gfs"
```

There are no implicit entrypoint filenames.

---

## 11. `[exports]`

`[exports]` declares public module export addresses.

```toml
[exports]
"vector" = "src/vector.gfs"
"matrix" = "src/matrix.gfs"
"geometry/rect" = "src/geometry/rect.gfs"
```

An export address is a public module address. It is not an internal namespace and does not create namespace mixing inside the module.

A module can export multiple public surfaces, but each one is explicit.

No top-level item becomes public merely by existing.

---

## 12. `[alias]`

`[alias]` declares local import shortcuts.

```toml
[alias]
math = "src/math.gfs"
shared = "../shared/src/main.gfs"
physics = "modules/physics.gfp"
```

The `$` prefix is used only at the import site, not in TOML.

```galfus
import math from "$math"
```

Aliases are local to the project/module configuration unless a future workspace policy explicitly shares them.

---

## 13. `[dependencies]`

`[dependencies]` declares dependencies using a `target` field.

The dependency declaration model is intentionally uniform: the dependency name is local, and the `target` field points to where or how the dependency is resolved.

```toml
[dependencies]
collection = { target = "collection" }
math = { target = "@vulppi/math-core/vector", version = "1.0.0" }
local_shared = { target = "../shared" }
physics = { target = "./modules/physics.gfp" }
audio = { target = "./modules/audio.gfp" }
```

The `target` field may point to:

```txt
builtin module name
organization-qualified module address
unqualified module address
workspace member
relative local path
.gfp proxy descriptor
future registry/cache source
```

Dependency declaration does not create a separate resolution model. It uses the same deterministic module resolution rules.

---

## 14. Import Address Model

Valid import address forms:

```txt
$alias
@organization/module/export/path
module/export/path
./relative/path
../relative/path
builtin-name
```

Examples:

```galfus
import math from "$math"
import user from "./user"
import vectors from "@vulppi/math-core/vector"
import collection from "collection"
```

All imports resolve to Galfus module descriptors:

```txt
.gfs
.gfp
```

Imports never load arbitrary files directly.

---

## 15. Valid Module Descriptors

Valid module descriptors are:

```txt
.gfs  Galfus source module
.gfp  Galfus proxy descriptor
```

Invalid direct import targets include:

```txt
debug maps
.wasm
.dll
.so
.dylib
.json
.toml
.png
.jpg
```

External payloads can still be used, but only through `.gfp` proxy descriptors and adapter policies.

---

## 16. Artifact Roles

### `.gfs`

Project source file. Used by compiler/tooling.

`.gfs` is not publishable.

### `.gfp`

Galfus proxy descriptor for an external payload, adapter, or bridge.

`.gfp` is publishable.

---

## 17. Galfus Proxy Files

A `.gfp` file describes how an external payload is exposed as a Galfus module/proxy.

It may describe:

```txt
payload path
payload kind
ABI or bridge type
exported adapter functions
ownership/resource policy
allowed targets
integrity metadata
```

A `.gfp` is isolated. It does not depend on `.gfs` source at runtime. It allows external functionality to be represented as a Galfus module descriptor without making arbitrary payloads importable.

---

## 18. Adapter Payloads

External payloads include files such as:

```txt
.wasm
.dll
.so
.dylib
```

They are not imported directly. They are referenced by `.gfp` files and included in final distribution units only if reached and allowed by target/policy.

In the final bundle, a `.gfp` may disappear as a standalone file. Its information becomes a minimal adapter descriptor, manifest metadata, and embedded payload data if the payload is used.

---

## 19. Internal Debug Mapping

Debug mapping data contains debug and tooling data.

They may include:

```txt
source spans
local names
module paths
symbol names
reconstruction metadata
IDE/autocomplete paths
debug mapping
```

This mapping data is not required for release execution.

---

## 20. Workspace / Monorepo Layout

A workspace can group multiple Galfus module projects.

```txt
my-workspace/
  galfus.toml
  workspace/
    engine/
      galfus.toml
      src/
      modules/
      build/
    game/
      galfus.toml
      src/
      modules/
      build/
    editor/
      galfus.toml
      src/
      modules/
      build/
  modules/
  build/
```

The root `galfus.toml` can declare workspace members explicitly.

```toml
[workspace]
members = [
  "workspace/engine",
  "workspace/game",
  "workspace/editor",
]
```

Members are not discovered by magical directory scanning. Workspace membership is explicit.

---

## 21. Workspace Members

Each workspace member has its own `galfus.toml`.

Each member remains an independent Galfus module project with its own `src/`, `modules/`, and `build/` directories.

Workspace membership gives the resolver a deterministic way to connect local modules during development.

---

## 22. Deterministic Resolution

For the same workspace state, configuration, lock file, and inputs, resolution must produce the same module graph.

Suggested resolution precedence:

```txt
1. local aliases
2. relative imports
3. workspace members
4. local modules directory
5. registry/cache dependencies
6. builtins
```

Ambiguous imports are errors.

A project or workspace policy may override priority only if the override is explicit and deterministic.

---

## 23. Case Sensitivity

All workspace paths, import addresses, aliases, module names, export addresses, and dependency identifiers are case-sensitive.

This applies even on host filesystems that are case-insensitive.

For example:

```txt
./User
./user
```

These are different addresses.

A resolver running on a case-insensitive filesystem must still preserve and validate casing to keep builds deterministic across platforms.

---

## 24. Module Identity

Module identity is derived from:

```txt
organization, if present
module name
version
export address, when resolving an exported surface
artifact kind
integrity metadata, when resolved from locked/published artifacts
```

A public module address may be organization-qualified:

```txt
@vulppi/math-core/vector
```

or unqualified:

```txt
math-core/vector
```

---

## 25. Builtins and Standard Modules

Builtins are modules provided by the Galfus distribution. They are not mandatory runtime components, and are split into:

- **`std/*` (Thin Target Standard Surface)**: Low-level modules (such as `std/io`, `std/fs`, `std/net`, `std/time`, `std/env`, `std/random`, and `std/process`) that interface directly with host capabilities. Access is restricted under a closed sandbox by default.
- **Rich Utility Modules**: Target-agnostic modules (such as `text`, `format`, `json`, `regex`, `math`, `path`, `http`, `collections`, and `crypto`) that compile to target-agnostic instructions or wrap low-level `std/*` calls.

Detailed specifications for builtins are documented in [Galfus Builtins Reference](../05-adapters_and_builtins/01-builtins_reference.md).

Builtin modules are included only if used/reached, or if an explicit target/policy requires them.

---

## 26. Compiler Module

The optional compiler module is named:

```txt
compiler
```

It may expose compiler functionality such as:

```txt
lexer
parser
resolver
checker
ownership checker
MIR builder
bytecode writer
Module Image builder
executable serialization writer
debug map writer
```

Normal apps do not include `compiler`. REPLs, playgrounds, sandboxes, hot reload hosts, and development tools may include it.

---

## 27. Build Behavior

A build resolves the workspace graph, compiles source modules, validates dependencies, and emits artifacts.

Conceptual flow:

```txt
.gfs / .gfp discovery
  -> module records
  -> export surfaces
  -> dependency resolution
  -> external payload references
  -> adapter descriptors
  -> validation
  -> compiler pipeline for .gfs
  -> Galfus Module Image
  -> target bundle serialization
```

Build inputs can include:

```txt
.gfs
.gfp
payloads referenced by .gfp
galfus.toml
galfus.lock, when present
```

Build outputs can include:

```txt
final bundle blob
cache
diagnostics
logs
```

---

## 28. Debug and Release Builds

A debug build emits:

```txt
final bundle blob (with internal debug markers)
internal debug map
```

A release build emits the minimum final bundle blob required for execution and integrity.

Debug map data is not embedded into the final bundle blob by default.

---

## 29. Bundle Behavior

Bundling starts from an entrypoint or exported surface and produces a final distribution unit.

Conceptual flow:

```txt
entry/export
  -> BundleGraph
  -> reachability analysis
  -> module inclusion
  -> builtin inclusion
  -> adapter selection
  -> external payload inclusion
  -> tree-shaking
  -> manifest/integrity metadata
  -> single distribution unit
```

The bundle includes only what is used, reachable, or required by explicit policy.

---

## 30. Single Distribution Unit

Every final bundle is a single distribution unit.

Possible forms:

```txt
final bundle blob
executable
firmware .bin
APK
AAB
AAR
app bundle
WASM package
native archive/package
```

Single distribution unit does not necessarily mean one internal file.

APK, AAB, AAR, app bundles, and WASM packages may contain internal files. They are still treated as one final distribution unit.

---

## 31. Reachability and Tree-Shaking

All modules are loaded/included only if used.

This applies to:

```txt
user modules
builtin modules
compiler module
adapter modules
platform modules
external payloads
debug hooks
JIT hooks
owner graph extras
```

Exported does not mean bundled. Exported means available. Bundled means reached from the entry/export/host policy graph.

Tree-shaking removes unused:

```txt
modules
exports
functions
types
adapters
payloads
hooks
metadata
```

---

## 32. Sandbox Configuration

Sandboxing is configuration, not a runtime profile. By default, a Galfus program runs in a closed sandbox. Access to host resources via `std/*` modules is blocked unless explicit permission is granted in configuration.

Example `galfus.toml` configuration:

```toml
[sandbox]
max_memory = "64mb"
max_stack = "1mb"
max_steps = 10000000

[permissions]
"std/fs" = { read = ["/data/public", "./assets"], write = ["/data/temp"] }
"std/net" = { connect = ["api.example.com:443", "localhost:8080"] }
"std/env" = { allow_args = true, env_keys = ["PATH", "LANG"] }
"std/process" = { allow_exit = true }
```

Sandbox configuration may define:

```txt
memory limits
stack/call-depth limits
step/fuel limits
allowed adapters
allowed capabilities
host resource limits
std/* permissions (e.g. filesystem scopes, network hosts, environment keys)
```

These limits are especially useful for servers, playgrounds, REPLs, CI, and untrusted execution.

---

## 33. Runtime Profiles

Runtime profiles are not dependency policies.

They control execution strategy, optimization level, diagnostics, and debug tooling.

Profiles:

```txt
micro
fast
std
dev
```

They are incremental:

```txt
micro = minimum possible runtime
fast  = micro + quickening + aggressive JIT
std   = fast + balanced JIT + better panic messages and module traces
dev   = std + debug tools + debug trace + breakpoints
```

Runtime profile does not decide:

```txt
which modules are included
which adapters are included
whether sandbox is active
sandbox memory limits
host capabilities
```

Modules are included by usage/reachability. Adapters are selected by target, policy, and usage. Sandbox is configuration.

---

## 34. Adapter Policy

Adapters connect Galfus modules to platform or host functionality.

Adapter selection is based on:

```txt
target
policy
reachability
external payload descriptors
host capabilities
```

Adapters are not selected by runtime profile.

Examples:

```txt
desktop/server native adapters
WASM host adapters
Android adapters
Apple platform adapters
embedded HAL adapters
```

---

## 35. Target Toolchains

Targets may require external toolchains.

Examples:

```txt
Android SDK / NDK
Xcode / Apple toolchain
WASM toolchain
embedded SDK / HAL / firmware toolchain
native linker/package tools
```

Toolchain requirements are documented per target. They are not language semantics.

---

## 36. Runtime Compilation Workspaces

A workspace can include the `compiler` module when runtime compilation is needed.

Use cases:

```txt
REPL
playground
sandbox
hot reload host
LLM-assisted execution environment
development tool
```

The compiler module may generate a Galfus Module Image or final bundle blob while the VM is already running, if target and policy allow it.

Normal apps do not include the compiler module.

---

## 37. Build Cache

`build/` may contain cache data such as:

```txt
resolved module graph
MIR
bytecode
hashes
diagnostics
intermediate Module Images
bundler graph data
```

Cache must not change the final result. It can only accelerate repeated builds.

---

## 38. `galfus.lock`

A workspace can use a lock file:

```txt
galfus.lock
```

The lock file records concrete resolutions:

```txt
resolved dependency versions
resolved module identities
artifact hashes
registry/cache resolutions
external payload integrity data
```

`galfus.lock` helps make builds reproducible.

It does not replace `galfus.toml`; it records the exact result of resolution.

---

## 39. Generated Files Policy

Generated files belong in `build/`.

Project-owned source files belong in `src/`.

Local dependencies and payloads belong in `modules/` or in explicitly configured dependency targets.

---

## 40. IDE and Source Reconstruction Data

IDE/autocomplete data, debug maps, source spans, and source reconstruction metadata belong in:

```txt
internal debug maps
build/ cache
external tooling data
```

They do not belong in release final bundle blobs.

---

## 41. Publishing Model

Only these artifacts are publishable:

```txt
.gfp
```

Not publishable:

```txt
.gfs
```

`.gfs` is source-only and local to development.

Published dependencies therefore use `.gfp`.

---

## 42. Dependency Direction

Valid dependency direction:

```txt
.gfs -> .gfp
```

This means source can compile and reference proxy/external adapter descriptors.

Invalid direction:

```txt
.gfp -> .gfs
```

A `.gfp` is isolated. Runtime/published artifacts do not depend back on project source.

---

## 43. Validation Rules

Workspace validation errors include:

```txt
missing galfus.toml
invalid module name
invalid organization name
invalid version format
invalid target
missing app entry
library with no entry and no exports
broken alias
ambiguous import
case mismatch
invalid export address
export target missing
missing dependency descriptor (.gfp)
missing external payload referenced by .gfp
attempt to import unsupported file type directly
attempt to publish .gfs
attempt to treat debug map as importable module
invalid lock resolution
integrity/hash mismatch
```

Validation errors prevent a valid build or bundle from being produced.

---

## 44. Non-Goals

The workspace model does not provide:

```txt
implicit main/index/mod/root resolution
arbitrary import loaders
runtime source loading by default
direct import of .wasm/.dll/.so/.dylib
package-first semantics
operator/module namespace inference from export paths
publication of .gfs source files
monolithic standard library inclusion
```

---

## 45. Summary

```txt
Galfus workspaces are explicit, module-first, deterministic graphs.

galfus.toml defines identity, target kind, entry, exports, aliases, dependencies, and target configuration.

Imports resolve only to Galfus module descriptors: .gfs or .gfp.

.gfs is source-only and not publishable.

.gfp is a publishable artifact.

All final bundles are single distribution units.

All modules are included only if used/reached.

All paths and import addresses are case-sensitive.

Runtime profiles control execution strategy and diagnostics, not dependency inclusion.

Sandboxing is configuration.

Adapters are selected by target, policy, and reachability.

galfus.lock records concrete resolutions for reproducible builds.
```
