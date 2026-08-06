# Virtual Kernel

If `Virtual Threads` isolate memory and the virtual machine (VM) runs the *Bytecode*, who manages the queue and tells when everything should run? This is where the great host-agnostic orchestrator comes in: the **Virtual Kernel**.

## OS-Agnostic Orchestrator

The *Virtual Kernel* should not be confused with the Kernel of your base operating system (Linux/Windows/Mac). It is a lightweight lifecycle engine written entirely hardware-agnostic.

- It receives suspended tasks (`PendingContinuation`).
- It routes byte messages between Virtual Threads.
- It makes consistent and portable Scheduling decisions, dealing with chronological tie-breaks to ensure determinism.

## The Benefit of the Virtual Layer

The immense advantage of this design is **cross-determinism**.

Given that scheduler decisions are made by pure software in the Virtual Kernel — and not relegated to the underlying OS pthreads/Win32 threads —, the exact same package (`PackageImage`) is guaranteed to fire events and return asynchronies **in the same order**, whether it is running natively on Linux, compiled as a single script in a browser `WASM` environment blocking an Event Loop, or even on a bare-metal IoT chip with no OS at all.
