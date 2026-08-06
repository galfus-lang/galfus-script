# Ownership Graph

One of the most striking features of the Galfus ecosystem is its approach to memory management.
Galfus **does not have a traditional Garbage Collector (GC)** that runs in cycles looking for orphaned objects in memory.

## Deterministic Management

Instead of relying on the unpredictable latency of a background collector (or Mark-and-Sweep cycles), the language was architected under an **Ownership Graph** model.

- Every resource instantiated or allocated in Galfus has exact traceability of who "owns" it.
- The release of a resource follows a trail through the graph, unattached to global "cycles".
- When the root variable holding the ownership of a data instance goes out of scope (or undergoes destructive mutation without passing ownership), the language deallocates the underlying structure instantly and deterministically.

## Practical Impact

1. **Absolute Predictability**: The virtual machine will never make sudden pauses ("Stop the World") to clean up garbage. This is critical for embedded environments (Firmwares, RTOS) or physical simulators/games.
2. **Clear Lifecycle**: If complex reference cycles exist, they must be broken semantically or by using weak references, forcing the programmer to design healthy data models at the source, eliminating invisible burdens on the host.
3. **Constant Isolation**: Deallocation occurs in parallel with the normal execution of the *Virtual Thread*, keeping the memory footprint (RAM consumption) as small as possible by discarding garbage as soon as it becomes obsolete.
