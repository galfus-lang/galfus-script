# Galfus Adapters Surface Reference

> **Status: Planned design.** The implemented host boundary uses the optional
> asynchronous `HostProvider` through the `Instruction::CallNative` mechanism. Proxy descriptors, native/WASM adapters, payloads,
> and bundle reachability described below are not implemented yet.

This document defines the Galfus adapter surface model.

It records the architectural decisions for exposing external native, WASM, mobile, embedded, and host capabilities to Galfus without exposing raw pointers, arbitrary memory, or unsafe host APIs to normal Galfus source code.

The adapter model is based on typed module surfaces, `.gfp` proxy descriptors, opaque resource types, explicit capabilities, ownership/resource policy, deterministic reachability, and runtime validation.

---

## Table of Contents

1. [Purpose](#1-purpose)
2. [Core Principle](#2-core-principle)
3. [Adapter Surface](#3-adapter-surface)
4. [Galfus Proxy Files](#4-galfus-proxy-files)
5. [External Payloads](#5-external-payloads)
6. [Opaque Resource Types](#6-opaque-resource-types)
7. [Non-Constructible Opaque Structs](#7-non-constructible-opaque-structs)
8. [No Generic Opaque Pointer](#8-no-generic-opaque-pointer)
9. [Runtime Resource Handles](#9-runtime-resource-handles)
10. [C-ABI Mapping](#10-c-abi-mapping)
11. [WASM Adapter Mapping](#11-wasm-adapter-mapping)
12. [Ownership and Resource Policy](#12-ownership-and-resource-policy)
13. [Memory Builtins and Memory Safety](#13-memory-builtins-and-memory-safety)
14. [Capability and Sandbox Policy](#14-capability-and-sandbox-policy)
15. [Reachability and Bundling](#15-reachability-and-bundling)
16. [Validation Rules](#16-validation-rules)
17. [Example: Native Physics Adapter](#17-example-native-physics-adapter)
18. [Security Guarantees](#18-security-guarantees)
19. [Design Advantages](#19-design-advantages)
20. [Non-Goals](#20-non-goals)
21. [Summary](#21-summary)

---

## 1. Purpose

Adapters connect Galfus code to external capabilities.

External capabilities may include:

```txt
native libraries
C ABI functions
WASM modules
host APIs
mobile platform APIs
embedded HALs
engine subsystems
GPU resources
audio systems
physics systems
file handles
network handles
external memory buffers
```

The goal is to allow Galfus code to use external systems while preserving:

```txt
type safety
resource safety
ownership validation
runtime validation
sandbox policy
capability policy
build determinism
bundle reachability
no raw pointer exposure
```

Adapters are not arbitrary dynamic loaders. They are explicit, typed, policy-controlled module surfaces.

---

## 2. Core Principle

Galfus source code never manipulates raw pointers.

The core rule is:

```txt
Galfus sees typed resource values.
The runtime sees validated resource handles.
The adapter sees native pointers, WASM handles, or host objects.
External code sees its own ABI-level representation.
```

Normal Galfus code cannot:

```txt
create raw pointers
cast values into pointers
cast pointers into integers
do pointer arithmetic
dereference native memory directly
forge external handles
construct adapter-owned opaque resources directly
import .dll/.so/.dylib/.wasm payloads directly
```

Interop is done through typed adapter functions and opaque resource types.

---

## 3. Adapter Surface

An adapter surface is the Galfus-facing API exposed by an external capability.

It may expose:

```txt
functions
constants
type aliases
enums
choices
opaque resource structs
safe buffer/view types
adapter-specific errors
adapter-specific constraints
```

Example Galfus-facing surface:

```galfus
external struct PhysicsWorld {}
external struct PhysicsBody {}

fn createWorld(): PhysicsWorld
fn createBody(world: PhysicsWorld): PhysicsBody
fn step(world: PhysicsWorld, delta: f32): null
fn applyForce(body: PhysicsBody, x: f32, y: f32): null
fn destroyBody(body: PhysicsBody): null
```

This surface is not ordinary `.gfs` source authored by the application. It is produced from or declared by a `.gfp` descriptor and validated by the build/tooling pipeline.

---

## 4. Galfus Proxy Files

A `.gfp` file describes an external payload, adapter, bridge, or host binding.

A `.gfp` may define:

```txt
adapter module identity
exported Galfus-facing surface
opaque resource types
external function symbols
ABI or bridge kind
parameter and return mappings
payload location
payload kind
allowed targets
required capabilities
resource ownership policy
resource drop/finalize behavior
memory access policy
integrity metadata
version requirements
```

A `.gfp` is the authority for external resource types.

Normal `.gfs` source does not define adapter-owned opaque resources. It imports them from a resolved adapter surface.

---

## 5. External Payloads

External payloads may include:

```txt
.wasm
.dll
.so
.dylib
.a
frameworks
platform APIs
embedded drivers
host-provided services
```

External payloads are not direct Galfus imports.

Invalid direct imports:

```galfus
import physics from "./libphysics.so"  // invalid
import audio from "./audio.wasm"       // invalid
```

Valid import through a proxy/adapter descriptor:

```galfus
import physics from "./modules/physics.gfp"
```

or through a configured dependency:

```toml
[dependencies]
physics = { target = "./modules/physics.gfp" }
```

```galfus
import physics from "physics"
```

The final bundle may embed the required external payload or describe the required host capability, depending on target and policy.

---

## 6. Opaque Resource Types

An opaque resource type is a Galfus nominal type representing an external resource.

Examples:

```galfus
external struct PhysicsWorld {}
external struct PhysicsBody {}
external struct TextureHandle {}
external struct AudioStream {}
external struct NativeWindow {}
external struct GpuBuffer {}
external struct FileHandle {}
```

Opaque resource types have no Galfus-visible fields.

They are used to give the type checker a precise symbol for external resources without exposing their memory representation.

The following is intentionally not exposed:

```txt
native pointer value
native address
WASM linear-memory pointer
host object reference
internal runtime resource id
adapter table index
```

Galfus code sees only a typed value.

---

## 7. Non-Constructible Opaque Structs

Opaque resource structs cannot be constructed directly by ordinary Galfus source code.

Invalid:

```galfus
var body = new(PhysicsBody) {}
```

Valid:

```galfus
var world = physics::createWorld()
var body = physics::createBody(world)
```

Only an adapter function, host bridge, or runtime-authorized operation may produce a value of an adapter-owned opaque resource type.

This prevents handle forgery.

An opaque resource type is therefore:

```txt
nominal
fieldless
adapter-defined
non-constructible by .gfs
non-forgeable by user code
runtime-backed
policy-controlled
```

---

## 8. No Generic Opaque Pointer

Galfus must not expose a universal user-facing type such as:

```galfus
struct OpaquePointer {}
```

A generic opaque pointer would allow accidental or unsafe mixing of unrelated resources.

Invalid design:

```galfus
struct OpaquePointer {}

fn render(texture: OpaquePointer): null
fn step(body: OpaquePointer): null
```

Preferred design:

```galfus
external struct TextureHandle {}
external struct PhysicsBody {}

fn render(texture: TextureHandle): null
fn step(body: PhysicsBody): null
```

The type checker must reject incompatible resource usage:

```galfus
var body = physics::createBody(world)
render(body) // semantic error: PhysicsBody is not TextureHandle
```

Each external resource has its own symbol.

---

## 9. Runtime Resource Handles

At runtime, an opaque resource value is represented by a VM-managed handle, not by a raw pointer visible to Galfus code.

Conceptual representation:

```txt
Galfus value:
  PhysicsBody

VM value:
  ResourceHandle {
    type_id,
    resource_id,
    generation,
    adapter_id,
    ownership_flags
  }

Adapter table:
  resource_id + generation -> native pointer / WASM handle / host object
```

The exact binary representation is implementation-defined, but it must support:

```txt
type validation
liveness validation
adapter ownership validation
generation or stale-handle protection
resource drop/finalize scheduling
weak invalidation
sandbox/capability checks
```

Galfus source code cannot observe or construct this representation.

---

## 10. C-ABI Mapping

Typed Galfus values can map to C ABI values through adapter-defined rules.

Typical primitive mappings:

```txt
bool    -> adapter-defined bool representation, usually uint8_t or _Bool
i8    -> int8_t
i16   -> int16_t
i32   -> int32_t
i64   -> int64_t
u8   -> uint8_t
u16  -> uint16_t
u32  -> uint32_t
u64  -> uint64_t
f32 -> float
f64 -> double
null    -> no payload or nullable marker according to ABI policy
```

Typical aggregate/resource mappings:

```txt
opaque resource -> void* / typed native pointer / opaque native handle
array/view      -> pointer + length + element layout policy
buffer          -> pointer + length + ownership/access policy
struct repr     -> explicit ABI layout only when declared ABI-safe
choice repr     -> tag + payload only when declared ABI-safe
```

C-ABI conversion is performed by the adapter boundary.

Galfus code does not perform the conversion itself.

The adapter must validate:

```txt
resource type matches expected parameter type
resource is alive
resource belongs to the expected adapter or compatible adapter family
resource capability is allowed
ownership/borrow policy allows the call
memory view access is valid for the requested operation
```

---

## 11. WASM Adapter Mapping

WASM adapters follow the same surface rules as native adapters.

A Galfus opaque resource may map to:

```txt
WASM externref
WASM table reference
WASM linear-memory handle
host-managed resource id
adapter-managed handle
```

WASM linear memory must not be exposed as arbitrary raw pointer manipulation in normal Galfus code.

Safe access should use explicit buffer/view types with adapter-defined permissions:

```txt
read-only view
write-only view
read-write view
copy-in buffer
copy-out buffer
borrowed buffer
owned buffer
```

The adapter must define how memory is copied, borrowed, pinned, invalidated, or released.

---

## 12. Ownership and Resource Policy

Opaque resources participate in the Galfus ownership model.

A `.gfp` may define resource policies such as:

```txt
owned
borrowed
host-owned
adapter-owned
manual-drop
auto-drop
weak-observable
non-weakable
copy-forbidden
move-only
clone-through-adapter
```

Conceptual examples:

```txt
owned:
  Galfus value owns the external resource.
  When the value is released, the adapter finalizer runs.

host-owned:
  Host owns the resource.
  Galfus observes or references it but does not destroy it.

borrowed:
  Resource is valid only within a call or scoped lifetime.

manual-drop:
  User must call an explicit destroy function.
  Runtime may still invalidate handles after destroy.

auto-drop:
  Runtime schedules drop/finalize when the owner graph releases the value.
```

The Owner Graph Core coordinates:

```txt
anchors
edges
weak observers
release scheduling
drop/finalize scheduling
cycle handling
weak invalidation
```

External resources must not bypass lifetime validation.

---

## 13. Memory Builtins and Memory Safety

Memory-related builtins may exist, but they must not expose raw unchecked pointer access to normal Galfus code.

Preferred split:

```txt
memory_safe:
  buffers
  slices
  typed views
  byte views
  copy operations
  bounds-checked access
  adapter-approved memory sharing

memory_unsafe:
  privileged operations
  capability-gated
  sandbox-restricted
  unavailable by default
```

Normal Galfus code may use safe memory abstractions such as:

```galfus
var bytes: [u8] = "hello"
```

or adapter-defined safe views:

```galfus
var pixels = image::lockPixels(texture)
image::unlockPixels(texture, pixels)
```

The runtime/adapter must ensure:

```txt
bounds are known or checked
lifetime is valid
borrow rules are respected
views cannot outlive owners unless policy allows it
stale views are invalidated
write access requires permission
host memory cannot be arbitrarily scanned or mutated
```

Unsafe memory operations, if they exist, are not part of normal application semantics.

---

## 14. Capability and Sandbox Policy

Adapters are controlled by capabilities and sandbox configuration.

A host or bundle policy may define:

```txt
allowed adapters
allowed functions
allowed resource kinds
maximum memory
maximum stack
maximum steps/fuel
allowed filesystem access
allowed network access
allowed native libraries
allowed WASM modules
allowed external payloads
```

An adapter call is valid only if:

```txt
the module was resolved
the function is exported by the adapter surface
the call is semantically well typed
the resource handles are valid
the capability policy allows it
the sandbox policy allows it
the target policy allows it
the adapter/resource ownership policy allows it
```

Capability checks are not optional for untrusted or sandboxed execution.

---

## 15. Reachability and Bundling

Adapters are included only when used, reached, or required by explicit policy.

A bundle starting from an entrypoint includes:

```txt
reachable Galfus modules
reachable adapter surfaces
reachable opaque resource types
reachable adapter functions
reachable external payload descriptors
reachable external payloads when policy allows embedding
required integrity metadata
required manifest metadata
```

Unused adapters, unused functions, unused resource types, unused payloads, and unused debug hooks must be removed from release bundles when possible.

Exported does not mean bundled.

```txt
exported = available to dependents
bundled  = reachable from entry/export/policy graph
```

---

## 16. Validation Rules

Validation errors include:

```txt
.gfs attempts to directly construct an adapter-owned opaque resource
.gfs attempts to import a raw external payload directly
.gfs attempts to use a resource with the wrong nominal type
.gfs attempts to cast a resource to a primitive pointer/integer representation
.gfs attempts to access fields of an opaque resource
.gfs attempts pointer arithmetic
.gfs attempts to call an adapter function without required capability
.gfp defines duplicate resource symbols
.gfp defines an ABI mapping that is not supported by the target
.gfp defines an unsafe memory view without required policy
.gfp resource policy conflicts with function signatures
.gfp external payload is missing
.gfp payload integrity check fails
adapter function receives a stale or invalid resource handle
adapter function receives a resource owned by an incompatible adapter
adapter call violates sandbox policy
adapter call violates ownership/borrow policy
```

Validation may occur at multiple layers:

```txt
workspace resolution
.gfp validation
semantic checking
ownership checking
Module Image generation
executable image serialization validation
runtime loading
adapter call dispatch
sandbox/capability enforcement
```

---

## 17. Example: Native Physics Adapter

Example dependency declaration:

```toml
[dependencies]
physics = { target = "./modules/physics.gfp" }
```

Example Galfus use:

```galfus
import physics from "physics"

fn main(): null {
  var world = physics::createWorld()
  var body = physics::createBody(world)

  physics::applyForce(body, 0.0, 10.0)
  physics::step(world, 0.016)

  return
}
```

Conceptual adapter surface produced by `physics.gfp`:

```galfus
external struct PhysicsWorld {}
external struct PhysicsBody {}

fn createWorld(): PhysicsWorld
fn createBody(world: PhysicsWorld): PhysicsBody
fn applyForce(body: PhysicsBody, x: f32, y: f32): null
fn step(world: PhysicsWorld, delta: f32): null
```

Conceptual C ABI target:

```c
void* physics_create_world(void);
void* physics_create_body(void* world);
void physics_apply_force(void* body, float x, float y);
void physics_step(void* world, float delta);
```

Conceptual runtime flow:

```txt
physics::createWorld()
  -> adapter calls physics_create_world()
  -> native returns void*
  -> runtime stores pointer in adapter resource table
  -> Galfus receives PhysicsWorld resource handle

physics::applyForce(body, 0.0, 10.0)
  -> VM validates body is PhysicsBody
  -> VM validates resource is alive
  -> VM validates capability/sandbox policy
  -> adapter resolves native void*
  -> adapter calls physics_apply_force(void*, float, float)
```

The Galfus code never sees the `void*`.

---

## 18. Security Guarantees

The adapter surface model provides these guarantees when implemented correctly:

```txt
no raw pointer exposure to Galfus source
no pointer arithmetic in Galfus source
no direct dereference of external memory
no direct construction of adapter-owned resources
no generic universal pointer type
nominal type safety for each external resource kind
runtime liveness validation
stale handle protection
ownership/drop policy enforcement
adapter capability enforcement
sandbox enforcement
reachability-based inclusion of adapters and payloads
```

This makes native/WASM interop compatible with Galfus safety goals.

---

## 19. Design Advantages

The adapter surface model gives Galfus a strong position as a typed VM-first host language.

Advantages:

```txt
low-overhead native interop
safe opaque resources
portable WASM/native adapter model
strong type checking across external calls
controlled plugin systems
engine-friendly scripting
LLM-friendly generated code validation
sandboxed execution of user code
compact bundle-centered distribution
explicit external capability model
```

This is especially useful for:

```txt
game engines
plugin hosts
editors
modding systems
REPL/playground environments
LLM-assisted code execution
sandboxed automation
portable app logic
controlled native extension surfaces
```

---

## 20. Non-Goals

The adapter surface model does not provide:

```txt
raw pointer values in normal Galfus code
pointer arithmetic
unchecked native memory access
arbitrary dynamic library loading by source code
arbitrary WASM loading by source code
direct import of .dll/.so/.dylib/.wasm
universal OpaquePointer type
forged external handles
implicit host API access
bypassing sandbox policy
bypassing ownership validation
bypassing capability checks
```

Galfus is not intended to replace Rust, C, or C++ for implementing low-level native systems.

Galfus is intended to provide a safe, typed, portable, VM-first layer above those systems.

---

## 21. Summary

Galfus adapters expose external capabilities through typed, explicit, policy-controlled surfaces.

The core model is:

```txt
.gfp defines the adapter surface.
.gfp owns opaque resource type symbols.
Opaque resource structs are fieldless and non-constructible by .gfs.
Each external resource has its own nominal type.
There is no generic OpaquePointer.
The VM stores resource handles, not user-visible raw pointers.
The adapter converts handles to native pointers, WASM resources, or host objects.
Ownership and drop behavior are governed by resource policy.
Sandbox and capability policy govern whether calls are allowed.
Bundling includes only reachable adapters and payloads.
```

Compact form:

```txt
Galfus sees typed resources.
Runtime validates handles.
Adapters access pointers/host resources.
External payloads stay behind policy.
```

This allows Galfus to support C-ABI and WASM interop with low overhead while preserving the safety, determinism, ownership, and sandbox goals of the language.
