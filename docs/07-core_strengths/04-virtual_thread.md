# Virtual Thread

In the Galfus ecosystem, running code rarely talks directly to the hardware abstraction on which it is spinning. Isolation begins through the conceptual foundation of the **Virtual Thread**.

## "Shared-Nothing" Model

A _Virtual Thread_ is an isolated execution unit.
This means that, unlike most languages where threads share the same global _Heap_, each virtual thread in Galfus has its own instance cycle. No Thread A can access Thread B's memory (not even under _locks_).

The only way Virtual Threads talk to each other, or interact externally, is through dispatches called **ByteMessage**. All communication crosses the boundary of byte serialization or atomic transfers (pass-by-value) controlled by the orchestrator.

## Massive Advantages in Hostile Environments

This architectural foundation makes Galfus a unique tool for:

### Embedded Systems and RTOS (Real-Time OS)

- Because the _Virtual Thread_ is fully aware of its stack size and local memory footprint, there is no risk of asynchronous conflicts. Real-time operating systems (RTOS) can schedule Galfus Virtual Threads forecasting strict time constraints and predetermined memory bounds with complete isolation. Without cross-hardware corruption, predictability reigns absolute.

### Web Workers (Browsers)

- JavaScript Web Workers are known to be heavy and limited to `postMessage` communication.
- The Virtual Thread isolation fits this model perfectly! We can map a _Galfus Virtual Thread_ directly to a _Web Worker_. Since the threads were already written without assuming shared memory (and use the isolated `ByteMessage` payload), porting a native embedded codebase or a backend to the Web runs smoothly, for free, without the famous lock problems and without needing _SharedArrayBuffer_.
