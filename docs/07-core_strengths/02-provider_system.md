# Provider System

In Galfus, the language and the virtual machine are designed to be perfectly isolated. The base infrastructure assumes no access to the File System (FS), Network, or even the screen (visual or text I/O).

Everything that connects the executable code of a `.gfs` to the "Capabilities" (real-world capabilities) of the outside world is part of the **Provider System**.

## What is a Provider?

A *Provider* is a concrete implementation hosted directly by the Execution Host.
It is ideal for providing native language capabilities or modules that function almost like a **Standard Library**.

For example, the ability to print to the screen:
```galfus
import { println } from "std/io"

println("Log!")
```
Does not exist in the Galfus interpreter. The host must inject the `std::io` provider package into the *Workspace* during loading.

## Core Strengths

- **Absolute Security (Sandboxing)**: The host running the VM has final control over which Providers to install. You can run an untrusted script by injecting an empty and fake `std::io`, creating an impenetrable sandbox (test-bed) environment with no computational cost (hooks/VM flags).
- **No Injected Magic**: All internal properties are materialized in a host-agnostic way, so Galfus behaves exactly the same on a Web page as on a native desktop OS. If the Provider exists, the script runs. If it doesn't, compilation aborts early reporting the absence.
