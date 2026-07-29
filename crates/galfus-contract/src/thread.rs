use std::time;

/// Represents an encapsulated virtual thread, ready to run.
/// The host environment does not know its internals.
pub trait RunnableTask: Send {
    /// The host calls this method and provides a "budget" (e.g., number of instructions).
    /// The task runs until the budget is exhausted or it needs to pause.
    fn run(self: Box<Self>, budget: usize) -> ThreadResult;
}

/// The result returned after running a slice of a virtual thread.
pub enum ThreadResult {
    /// The thread consumed the budget but still has work to do.
    /// The host should re-queue it.
    Yielded(Box<dyn RunnableTask>),

    /// The thread finished execution successfully.
    Completed(i32),

    /// The thread encountered a critical error (panic).
    Failed(String),

    /// The thread needs to call a Provider or is waiting for a message.
    /// The Host should discard the task. The Runtime Orchestrator will
    /// wake it up and send it back to the Host when ready.
    Blocked { timeout: Option<time::Duration> },
}

/// The result returned after running one step of the executor.
pub enum ExecutorStepResult {
    /// The executor still has tasks in the queue or is actively running them.
    Running,
    /// All tasks are blocked, waiting for external I/O or a timeout.
    Blocked { timeout: Option<time::Duration> },
    /// All tasks have completed successfully. Contains the exit code of the entry thread.
    Completed(i32),
}

/// A unit of work for the kernel driver to execute.
pub enum KernelTask {
    /// Work that must be pinned to the main thread (e.g. Orchestrator loop)
    Main(Box<dyn RunnableTask>),
    /// Work that can run on any available background thread
    Any(Box<dyn RunnableTask + Send>),
}

/// The Host must implement this trait to dictate how tasks are scheduled.
pub trait KernelDriver: Send + Sync {
    /// The Kernel or Orchestrator calls this to submit work.
    fn dispatch(&self, task: KernelTask);

    /// Sets the callback to be invoked when the driver completes its execution.
    fn on_exit(&self, callback: Box<dyn Fn(Result<i32, String>) + Send + Sync>);

    /// Runs the driver loop. Behavior (blocking vs non-blocking) depends on the implementation.
    fn run(&self);

    /// Executes a single step, returning the current status.
    fn step(&self) -> Result<ExecutorStepResult, String> {
        unimplemented!("step is not implemented by default")
    }
}
