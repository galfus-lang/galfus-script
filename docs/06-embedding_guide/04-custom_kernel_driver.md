# Creating a Custom Kernel Driver

Galfus uses a host-agnostic virtual kernel to manage its lifecycle. However, executing tasks cooperatively on hardware requires a bridge: the **`KernelDriver`**.

While the engine ships with a simple `CooperativeDriver` out-of-the-box, real-world applications often need deep integration with existing event loops (like Tokio, an iOS runloop, or a Game Engine tick loop).

## The `KernelDriver` Trait

To create a custom executor, you must implement the `galfus_contract::KernelDriver` trait. The runtime will push `KernelTask` instances into your driver, and you are responsible for running them.

```rust
use std::collections::VecDeque;
use std::sync::Mutex;
use galfus_contract::{KernelDriver, KernelTask, ExecutorStepResult, ThreadResult, ExecutionFailure};

/// A custom FIFO executor
pub struct MyCustomDriver {
    queue: Mutex<VecDeque<KernelTask>>,
}

impl MyCustomDriver {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
        }
    }
}

impl KernelDriver for MyCustomDriver {
    /// Called when the VirtualKernel wants to schedule a new task
    fn dispatch(&self, task: KernelTask) {
        self.queue.lock().unwrap().push_back(task);
    }

    /// Used when a task should be prioritized (e.g. resuming after I/O)
    fn dispatch_front(&self, task: KernelTask) {
        self.queue.lock().unwrap().push_front(task);
    }

    /// Register a callback to be fired when the Galfus application terminates
    fn on_exit(&self, _callback: Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>) {
        // Store the callback safely if needed
    }

    /// Force completion (can be used to abruptly terminate the driver)
    fn complete(&self, _result: Result<i32, ExecutionFailure>) {
        // Trigger your exit callback
    }

    /// A synchronous execution block that runs continuously until blocked.
    fn run(&self) {
        loop {
            // Here you can integrate your own loop semantics.
            // For example, yielding back to Tokio or blocking.
            let step = self.step();
            match step {
                ExecutorStepResult::Running => continue,
                ExecutorStepResult::Blocked { timeout } => {
                    // If timeout is Some, wait up to that duration.
                    // Otherwise, wait indefinitely for an external event.
                    break;
                },
                ExecutorStepResult::Completed(_) => break,
            }
        }
    }

    /// Executes a single budget-limited step from the queue
    fn step(&self) -> ExecutorStepResult {
        let task_entry = self.queue.lock().unwrap().pop_front();

        let Some(task_entry) = task_entry else {
            return ExecutorStepResult::Blocked { timeout: None };
        };

        // Run the task for an arbitrary budget of instructions (e.g., 100 instructions)
        let result = match task_entry {
            KernelTask::Main(task) => task.run(100),
            KernelTask::Any(task) => task.run(100),
        };

        match result {
            // Task yielded gracefully or hit budget, it is usually re-queued internally
            // by the VirtualKernel and will appear back in `dispatch`.
            ThreadResult::Discarded => ExecutorStepResult::Running,

            // Task is blocked waiting for I/O or a timeout
            ThreadResult::Blocked { timeout } => ExecutorStepResult::Blocked { timeout },

            // The task finished completely (Main thread returned)
            ThreadResult::Completed(res) => {
                let code = if let Ok(galfus_contract::BoundaryValue::I32(c)) = res { c } else { 0 };
                ExecutorStepResult::Completed(code)
            }
        }
    }
}
```

## Integrating with Tokio or Game Engines

When using a game engine like Bevy or an async runtime like Tokio:

1. **Avoid blocking in `run()`**: Instead of looping endlessly, you can call `.step()` once per frame/tick inside your main game loop.
2. **Handle `ExecutorStepResult::Blocked`**: If `step` returns blocked, Galfus is waiting for an asynchronous event (like network). You can safely skip ticking Galfus until the timeout expires or an external provider calls `inject_system_response`, which will trigger a `dispatch` waking the driver up.
3. **Task Affinity**: `KernelTask::Main` MUST be executed on the application's main thread (essential for OpenGL/Windowing context). `KernelTask::Any` can safely be dispatched to worker thread pools.
