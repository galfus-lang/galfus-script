# Galfus Concurrency and Actor Model Architecture

This document defines the conceptual design for multithreading, concurrency, and the Actor-based message passing architecture of the Galfus runtime.

---

## 1. Concurrency Model (Virtual Threads & Actors)

To ensure the Galfus VM remains target-agnostic and capable of running across desktop, mobile, web, and embedded targets, concurrency is abstracted into **Virtual Threads** (Actors):

1. **`VirtualThread`**: Tracks independent execution states (instruction pointer, local stack, and local heap).
2. **`ThreadRegistry` & `Runtime`**: The Galfus Orchestrator manages the lifecycle of threads, holding queues for `Runnable` and `Blocked` states.
3. **`ThreadExecutor` (galfus-contract)**: The host environment maps virtual threads to target capabilities:
   * **Desktop/Server**: Spawns OS threads or uses a thread-pool.
   * **Web/WASM**: Cooperative green threads yielding to the browser/Node.js event loop.
   * **Embedded (Monocore)**: A simple cooperative, single-threaded round-robin micro-scheduler (`SingleThreadExecutor`).

---

## 2. 100% Isolated Memory Architecture

Memory is completely isolated per thread to guarantee safety, local garbage collection efficiency, and deterministic destruction without race conditions.

### Private Heap

* Owned exclusively by a single `VirtualThread`.
* Managed by the single-threaded, highly optimized `owner_graph_core`.
* Requires no mutexes, atomics, or cross-thread synchronization.
* Has no global heap or software transactional memory.

---

## 3. Communication via Message Passing (Mailbox)

Since there is no shared memory, threads communicate exclusively by sending and receiving messages, following the **Actor Model**.

### Core Concepts

* **Mailbox**: Every thread has a dedicated concurrent queue (Mailbox) where it receives messages.
* **Byte Sequences Only**: Mailboxes transport `[u8]` values. Structured values must be explicitly serialized before sending and deserialized after receiving.
* **Non-Blocking Send**: Sending a message never blocks the sender.
* **Blocking Receive**: Receiving a message blocks the thread (`ThreadResult::Blocked`) if its mailbox is empty. The thread is parked in the `BlockedQueue` until a message arrives.

### Syntax Demonstration

```galfus
fn worker(): null {
  loop {
    let msg = receive()
    println(msg)
  }
}

fn main(): i32 {
  // Spawn a new virtual thread (actor)
  let target_thread = spawn(worker)
  
  // Send bytes to the actor
  send(target_thread, "Hello from main!")
  
  return 0
}
```

---

## 4. Integration with I/O and the Host (Providers)

The Galfus Runtime uses the exact same messaging mechanism to integrate with the Host environment for asynchronous I/O.

* **Target 0 (Host)**: The thread ID `0` is reserved for the Host Environment.
* **Providers**: When a script calls an I/O function (like `read_file`), it sends a message to Target 0. The Runtime delegates this to the corresponding `Provider`.
* **Async Resumption**: The thread blocks (`BlockedQueue`). Once the host completes the native I/O operation (e.g., via `tokio` or Node.js), it injects the response message back into the thread`s Mailbox, waking it up (`RunnableQueue`).
