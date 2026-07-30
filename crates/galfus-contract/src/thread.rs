use std::time;

/// Represents an encapsulated virtual thread, ready to run.
/// The host environment does not know its internals.
pub trait RunnableTask {
    /// The host calls this method and provides a "budget" (e.g., number of instructions).
    /// The task runs until the budget is exhausted or it needs to pause.
    fn run(self: Box<Self>, budget: usize) -> ThreadResult;

    /// Returns this task as transferable work when its continuation is `Send`.
    /// Main-thread tasks keep the default `None` implementation.
    fn into_any_thread(self: Box<Self>) -> Option<Box<dyn RunnableTask + Send>> {
        None
    }
}

/// The result returned after running a slice of a virtual thread.
pub enum ThreadResult {
    /// The task has finished its slice or encountered an error.
    /// The Orchestrator will handle the lifecycle via events. The Host must drop the task.
    Discarded,

    /// The thread finished execution successfully.
    Completed(i32),

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

/// The only scheduling information visible to a kernel driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskAffinity {
    Main,
    Any,
}

impl KernelTask {
    pub const fn affinity(&self) -> TaskAffinity {
        match self {
            Self::Main(_) => TaskAffinity::Main,
            Self::Any(_) => TaskAffinity::Any,
        }
    }
}

/// The Host must implement this trait to dictate how tasks are scheduled.
pub trait KernelDriver {
    /// The Kernel or Orchestrator calls this to submit work.
    fn dispatch(&self, task: KernelTask);

    /// Sets the callback to be invoked when the driver completes its execution.
    fn on_exit(&self, callback: Box<dyn Fn(Result<i32, crate::ExecutionFailure>) + Send + Sync>);

    /// Runs the driver loop. Behavior (blocking vs non-blocking) depends on the implementation.
    fn run(&self);

    /// Receives the final result of a persistent execution.
    fn complete(&self, _result: Result<i32, crate::ExecutionFailure>) {}

    /// Executes a single step, returning the current status.
    fn step(&self) -> ExecutorStepResult {
        unimplemented!("step is not implemented by default")
    }
}
