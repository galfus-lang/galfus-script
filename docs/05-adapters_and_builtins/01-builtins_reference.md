# Galfus Builtins and Standard Library Reference

This document defines the Galfus standard library design, its API surfaces, and the permission/sandbox model.

---

## 1. Design Philosophy

The Galfus standard library is organized into three distinct tiers defined in `galfus-contract`:

```txt
+------------------------------------------------------------------+
|                    3. Bridge Modules (Optional)                  |
|    (std/io, std/net, std/fs, std/process, std/time, std/gpio...) |
+------------------------------------------------------------------+
                                  |
                                  v  uses fn(async) __provider_*
+------------------------------------------------------------------+
|                       Host OS / Platform APIs                    |
+------------------------------------------------------------------+

+------------------------------------------------------------------+
|                    2. Utility Modules (Universal)                |
|    (text, format, format/ansi)                                   |
+------------------------------------------------------------------+

+------------------------------------------------------------------+
|                 1. Internal Core Modules (VM Native)             |
|    (std/async, std/thread, std/math, std/constraints, std/iterable)|
+------------------------------------------------------------------+
```

1. **Internal Core Modules (`std/async`, `std/thread`, `std/math`, `std/constraints`, `std/iterable`)**
   - Always included by default in every workspace.
   - Execute entirely within VM engine isolation using `__internal_*` primitives without touching the host OS.

2. **Utility Modules (`text`, `format`, `format/ansi`)**
   - Pure Galfus Script algorithmic utilities.
   - Platform-agnostic, developer-friendly interfaces.

3. **Bridge Modules (`std/io`, `std/net`, `std/fs`, `std/process`, `std/time`, etc.)**
   - **Optional** host capabilities declared as atomic pairs (`HostProvider` + `.gfs` bridge source).
   - Functions connecting to native host operations MUST use explicit `fn(async) __provider_*` declarations.
   - Modules exist in a workspace ONLY when registered by the host via `galfus-workspace`. Missing bridges fail at compile-time.

---

## 2. Sandbox and Permission Model

By default, any Galfus program runs in a **Closed Sandbox**. Access to low-level host resources through `std/*` is restricted.

### Default Sandbox State

- Attempting to import or use a `std/*` module without explicit permissions in the configuration causes a compilation or link-time capability error, or a runtime panic if loaded dynamically.
- System inputs, outputs, files, networking, process controls, and environment variable accesses are entirely blocked by default.

### Workspace Permissions Configuration

Permissions are explicitly declared in the module's `galfus.toml` file under the `[permissions]` section.

Example configuration:

```toml
[permissions]
# Allow specific directory scopes for reading and writing
"std/fs" = { read = ["/data/public", "./assets"], write = ["/data/temp"] }

# Allow connections only to specified domains/ports
"std/net" = { connect = ["api.example.com:443", "localhost:*"] }

# Allow environment variables read access to specific keys, and passing command-line args
"std/env" = { allow_args = true, env_permitted = ["^APP_.+$", "i"] }

# Allow exit codes and target-level execution controls
"std/process" = { allow_exit = true }
```

### Permission Inheritance & Propagation

- When a Tier 2 module (like `http`) uses a Tier 1 module (like `std/net`), the VM checks the calling context's permissions.
- A library module cannot bypass the sandbox restrictions configured for the main application bundle. The lowest common denominator of permissions applies.

---

## 3. Tier 1: `std/*` (Thin Target Standard Surface)

### `std/io`

Basic console and standard input/output stream interaction.

`std/io` is resolved through an optional `HostProvider` via asynchronous native
calls. Compilation records its provider requirement; execution preflight then
requires a compatible `io` provider before the first instruction runs. Hosts
can still run packages without providers when those packages do not import a
provider bridge.

```galfus
# Read bytes from standard input until the delimiter is reached or EOF.
# The delimiter is not included in the returned bytes.
# An empty delimiter is invalid.
fn read(until: [u8] = "\n"): [u8]

# Write raw UTF-8 bytes to standard output.
fn print(text: [u8]): null

# Write raw UTF-8 bytes followed by a newline to standard output.
fn println(text: [u8]): null
```

### `std/thread`

Virtual threads execute isolated functions with an independent heap and a
mailbox. `createThread` only creates the thread; `spawn` starts it once and
returns whether the transition succeeded. A key is optional, but allows
retrieval through `getThread`.

```galfus
import {
  createThread,
  getMessage,
  getThread,
  hasMessages,
} from "std/thread"

fn worker(args: [[u8]]): i32 {
  if hasMessages() {
    const message = getMessage()
  }
  return 0
}

export fn main(args: [[u8]]): i32 {
  const thread = createThread(worker, "worker")
  thread::spawn()
  thread::wait()   // blocks main until worker exits
  return 0
}
```

```galfus
struct Thread {
  id: i64,
  key: [u8] | null,
}

fn createThread(func: fn([[u8]]): i32, key: [u8] | null = null): Thread
fn getThread(key: [u8]): Thread | null

fn hasMessages(): bool
fn getMessage(): [u8] | null

fn Thread::spawn(self, args: [[u8]] | null = null): bool
fn Thread::isRunning(self): bool
fn Thread::isExited(self): bool
fn Thread::exitReason(self): i32 | null
fn Thread::send(self, data: [u8]): bool
fn Thread::tryReceiveMessage(self, timeout: i32): [u8] | null
fn Thread::wait(self): i32 | null
```

`hasMessages` and `getMessage` inspect the mailbox of the current thread.
`getMessage` is non-blocking: it removes and returns the oldest message, or
returns `null` when the mailbox is empty. `tryReceiveMessage` is the
timeout-aware receive operation associated with a `Thread` handle.

`Thread::wait` suspends the calling thread until the target thread exits.
It returns the exit code (`i32`) on success, or `null` if the thread ID is
invalid. If the target has already exited when `wait` is called, the
calling thread resumes immediately with the stored exit code.

### `std/fs`

Direct filesystem access, mapped to OS level operations.

```galfus
external struct FileHandle {}

struct FileStat {
  size: u64,
  is_dir: bool,
  modified: i64,
  created: i64,
}

# Open file path with mode and flags. Returns a FileHandle or null on failure
fn open(path: [u8], flags: i32, mode: i32): FileHandle

# Read bytes from a specific offset into the buffer. Returns bytes read
fn read(file: FileHandle, offset: i64, buffer: [u8]): i32

# Write bytes to a specific offset. Returns bytes written
fn write(file: FileHandle, offset: i64, data: [u8]): i32

# Close the file handle, releasing resources
fn close(file: FileHandle): null

# Query metadata for a given path
fn stat(path: [u8]): FileStat
```

### `std/net`

Raw TCP and UDP networking. It is currently provided by the native host only.
Socket values are opaque `u64` identifiers scoped to one execution; they are
not OS file descriptors and must be closed explicitly.

```galfus
# TCP client operations
fn tcpConnect(host: [u8], port: u16): u64 | null
fn tcpRead(socket: u64, maxBytes: u32): [u8] | null
fn tcpWrite(socket: u64, data: [u8]): bool
fn tcpClose(socket: u64): bool

# UDP datagram operations
fn udpBind(host: [u8], port: u16): u64 | null
fn udpReceive(socket: u64, maxBytes: u32): ([u8], [u8], u16) | null
fn udpSendTo(socket: u64, host: [u8], port: u16, data: [u8]): bool
fn udpClose(socket: u64): bool
```

`tcpRead` and `udpReceive` wait for data without blocking other virtual
threads. `udpReceive` returns `(data, peerHost, peerPort)`.

### `std/http`

Single HTTP request/response operations. Available in the native and web hosts.

```galfus
type Header = ([u8], [u8])
type Response = (i32, [Header], [u8])

fn request(
  method: [u8],
  url: [u8],
  headers: [Header] = [],
  body: [u8] | null = null,
): Response | null
```

The response tuple is `(status, headers, body)`. HTTP failures return `null`.
In browsers, normal Fetch/CORS rules apply.

### `std/websocket`

WebSocket client operations. Available in the native and web hosts.

```galfus
fn connect(url: [u8]): u64 | null
fn receive(socket: u64): [u8] | null
fn send(socket: u64, data: [u8]): bool
fn close(socket: u64): bool
```

`receive` waits for the next text or binary message and returns `null` after a
closed or invalid socket. See [Network Providers](./03-network_providers.md)
for platform support and examples.

### `std/time`

System-level and high-resolution timer access.

```galfus
# Return UTC UNIX timestamp in milliseconds
fn now(): i64

# Return monotonic time in nanoseconds/microseconds (for performance tracking)
fn monotonic(): i64

# Return system-specific timer ticks
fn ticks(): i64
```

### `std/env`

Process environment and runtime arguments.

```galfus
# Return list of command line arguments
fn args(): [[u8]]

# Return value of environment variable key, or null if unset
fn get(key: [u8]): [u8]

# Return current working directory path
fn cwd(): [u8]
```

### `std/random`

Secure target entropy access.

```galfus
# Fill target buffer with cryptographically secure random bytes from host entropy
fn randomBytes(buffer: [u8]): null
```

### `std/process`

Process termination and control. (Available only on desktop/server targets).

```galfus
# Exit current process execution with the specified exit code status
fn exit(code: i32): null
```

### `std/async`

Asynchronous primitives and `Future<T>` definitions.

```galfus
struct Future<T> {
  id: i64,
}
```

---

## 4. Tier 2: Rich Utility Modules

These modules do not interact with the host OS directly unless using a configured and permitted `std/*` surface. They represent the main application programming API.

### `text`

Byte-level text utilities for UTF-8 `[u8]` arrays. Operations that inspect
characters currently operate on ASCII byte ranges.

- `fn length(s: [u8]): i32` - Returns the byte length.
- `fn concat(a: [u8], b: [u8]): [u8]` - Concatenates two byte arrays.
- `fn slice(s: [u8], start: i32, count: i32): [u8]` - Extracts a byte range.
- `fn repeat(s: [u8], n: i32): [u8]` - Repeats a byte array.
- `fn startsWith(s: [u8], prefix: [u8]): bool` - Checks a byte prefix.
- `fn endsWith(s: [u8], suffix: [u8]): bool` - Checks a byte suffix.
- `fn trimStart(s: [u8]): [u8]` / `fn trimEnd(s: [u8]): [u8]` / `fn trim(s: [u8]): [u8]` - Trims ASCII whitespace.
- `fn toUpper(s: [u8]): [u8]` / `fn toLower(s: [u8]): [u8]` - ASCII case mapping.

### `format`

Base-level deterministic string conversion.

```galfus
constraint Stringable {
  fn stringify(self): [u8]
}

fn stringify<T>(value: T): [u8]
fn parse<T>(s: [u8]): ParseResult<T>
```

`stringify` is a conceptual generic builtin that returns compact bytes for booleans, `null`, raw `[u8]`, concrete integer/float widths, and types implementing `Stringable`. Supported `T` types for `stringify<T>` are:

- `bool`
- `null`
- `[u8]`
- concrete integer widths (`i8`, `i16`, `i32`, `i64`, `i128`, `u8`, `u16`, `u32`, `u64`, `u128`)
- concrete float widths (`f32`, `f64`)
- any type satisfying the `Stringable` constraint

`parse<T>` is a compiler-specialized builtin that parses numeric and primitive values, returning a `ParseResult<T>` containing the parsed value or an error. Supported target types `T` are:

- `bool`
- concrete integer widths (`i8`, `i16`, `i32`, `i64`, `i128`, `u8`, `u16`, `u32`, `u64`, `u128`)
- concrete float widths (`f32`, `f64`)

### `math` (Implemented as `std/math`)

Standard mathematical functions.

- Constants: `PI` (3.14159...), `E` (2.71828...).
- Functions: `sin(x)`, `cos(x)`, `tan(x)`, `log(x)`, `pow(base, exp)`, `sqrt(x)`, `ceil(x)`, `floor(x)`, `round(x)`.

*(Note: Other utility modules like `json`, `regex`, `path`, `http`, `collections`, and `crypto` are planned but not currently implemented in the core engine's utility modules list).*
