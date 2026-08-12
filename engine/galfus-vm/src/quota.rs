use galfus_contract::{ExecutionFailureKind, LimitsMetadata, ResourceLimitKind};

#[derive(Debug, Clone)]
pub struct RuntimeQuota {
    limits: LimitsMetadata,
    heap_objects: usize,
    heap_bytes: usize,
    call_depth: usize,
    threads: usize,
    futures: usize,
    pending_requests: usize,
    mailbox_messages: usize,
    mailbox_bytes: usize,
    event_queue: usize,
    kernel_tasks: usize,
    runnable_threads: usize,
    external_handles: usize,
    timers: usize,
    pending_states: usize,
}

impl RuntimeQuota {
    pub fn new(limits: LimitsMetadata) -> Self {
        Self {
            limits,
            heap_objects: 0,
            heap_bytes: 0,
            call_depth: 0,
            threads: 0,
            futures: 0,
            pending_requests: 0,
            mailbox_messages: 0,
            mailbox_bytes: 0,
            event_queue: 0,
            kernel_tasks: 0,
            runnable_threads: 0,
            external_handles: 0,
            timers: 0,
            pending_states: 0,
        }
    }

    pub fn limits(&self) -> &LimitsMetadata {
        &self.limits
    }

    pub fn heap_objects(&self) -> usize {
        self.heap_objects
    }

    pub fn try_reserve_heap_objects(&mut self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.heap_objects;
        let limit = self.limits.max_heap_objects;
        if current.saturating_add(amount) > limit {
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::HeapObjects,
                current,
                requested: amount,
                limit,
            });
        }
        self.heap_objects += amount;
        Ok(())
    }

    pub fn release_heap_objects(&mut self, amount: usize) {
        self.heap_objects = self.heap_objects.saturating_sub(amount);
    }

    pub fn heap_bytes(&self) -> usize {
        self.heap_bytes
    }

    pub fn try_reserve_heap_bytes(&mut self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.heap_bytes;
        let limit = self.limits.max_heap_bytes;
        if current.saturating_add(amount) > limit {
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::HeapBytes,
                current,
                requested: amount,
                limit,
            });
        }
        self.heap_bytes += amount;
        Ok(())
    }

    pub fn release_heap_bytes(&mut self, amount: usize) {
        self.heap_bytes = self.heap_bytes.saturating_sub(amount);
    }

    pub fn call_depth(&self) -> usize {
        self.call_depth
    }

    pub fn try_reserve_call_depth(&mut self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.call_depth;
        let limit = self.limits.max_call_depth;
        if current.saturating_add(amount) > limit {
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::CallDepth,
                current,
                requested: amount,
                limit,
            });
        }
        self.call_depth += amount;
        Ok(())
    }

    pub fn release_call_depth(&mut self, amount: usize) {
        self.call_depth = self.call_depth.saturating_sub(amount);
    }

    pub fn threads(&self) -> usize {
        self.threads
    }

    pub fn try_reserve_threads(&mut self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.threads;
        let limit = self.limits.max_threads;
        if current.saturating_add(amount) > limit {
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::Threads,
                current,
                requested: amount,
                limit,
            });
        }
        self.threads += amount;
        Ok(())
    }

    pub fn release_threads(&mut self, amount: usize) {
        self.threads = self.threads.saturating_sub(amount);
    }

    pub fn futures(&self) -> usize {
        self.futures
    }

    pub fn try_reserve_futures(&mut self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.futures;
        let limit = self.limits.max_futures;
        if current.saturating_add(amount) > limit {
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::Futures,
                current,
                requested: amount,
                limit,
            });
        }
        self.futures += amount;
        Ok(())
    }

    pub fn release_futures(&mut self, amount: usize) {
        self.futures = self.futures.saturating_sub(amount);
    }

    pub fn pending_requests(&self) -> usize {
        self.pending_requests
    }

    pub fn try_reserve_pending_requests(&mut self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.pending_requests;
        let limit = self.limits.max_pending_requests;
        if current.saturating_add(amount) > limit {
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::PendingRequests,
                current,
                requested: amount,
                limit,
            });
        }
        self.pending_requests += amount;
        Ok(())
    }

    pub fn release_pending_requests(&mut self, amount: usize) {
        self.pending_requests = self.pending_requests.saturating_sub(amount);
    }

    pub fn mailbox_messages(&self) -> usize {
        self.mailbox_messages
    }

    pub fn try_reserve_mailbox_messages(&mut self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.mailbox_messages;
        let limit = self.limits.max_mailbox_messages;
        if current.saturating_add(amount) > limit {
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::MailboxMessages,
                current,
                requested: amount,
                limit,
            });
        }
        self.mailbox_messages += amount;
        Ok(())
    }

    pub fn release_mailbox_messages(&mut self, amount: usize) {
        self.mailbox_messages = self.mailbox_messages.saturating_sub(amount);
    }

    pub fn mailbox_bytes(&self) -> usize {
        self.mailbox_bytes
    }

    pub fn try_reserve_mailbox_bytes(&mut self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.mailbox_bytes;
        let limit = self.limits.max_mailbox_bytes;
        if current.saturating_add(amount) > limit {
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::MailboxBytes,
                current,
                requested: amount,
                limit,
            });
        }
        self.mailbox_bytes += amount;
        Ok(())
    }

    pub fn release_mailbox_bytes(&mut self, amount: usize) {
        self.mailbox_bytes = self.mailbox_bytes.saturating_sub(amount);
    }

    pub fn event_queue(&self) -> usize {
        self.event_queue
    }

    pub fn try_reserve_event_queue(&mut self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.event_queue;
        let limit = self.limits.max_event_queue;
        if current.saturating_add(amount) > limit {
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::EventQueue,
                current,
                requested: amount,
                limit,
            });
        }
        self.event_queue += amount;
        Ok(())
    }

    pub fn release_event_queue(&mut self, amount: usize) {
        self.event_queue = self.event_queue.saturating_sub(amount);
    }

    pub fn kernel_tasks(&self) -> usize {
        self.kernel_tasks
    }

    pub fn try_reserve_kernel_tasks(&mut self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.kernel_tasks;
        let limit = self.limits.max_kernel_tasks;
        if current.saturating_add(amount) > limit {
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::KernelTasks,
                current,
                requested: amount,
                limit,
            });
        }
        self.kernel_tasks += amount;
        Ok(())
    }

    pub fn release_kernel_tasks(&mut self, amount: usize) {
        self.kernel_tasks = self.kernel_tasks.saturating_sub(amount);
    }

    pub fn runnable_threads(&self) -> usize {
        self.runnable_threads
    }

    pub fn try_reserve_runnable_threads(&mut self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.runnable_threads;
        let limit = self.limits.max_runnable_threads;
        if current.saturating_add(amount) > limit {
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::RunnableThreads,
                current,
                requested: amount,
                limit,
            });
        }
        self.runnable_threads += amount;
        Ok(())
    }

    pub fn release_runnable_threads(&mut self, amount: usize) {
        self.runnable_threads = self.runnable_threads.saturating_sub(amount);
    }

    pub fn external_handles(&self) -> usize {
        self.external_handles
    }

    pub fn try_reserve_external_handles(&mut self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.external_handles;
        let limit = self.limits.max_external_handles;
        if current.saturating_add(amount) > limit {
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::ExternalHandles,
                current,
                requested: amount,
                limit,
            });
        }
        self.external_handles += amount;
        Ok(())
    }

    pub fn release_external_handles(&mut self, amount: usize) {
        self.external_handles = self.external_handles.saturating_sub(amount);
    }

    pub fn timers(&self) -> usize {
        self.timers
    }

    pub fn try_reserve_timers(&mut self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.timers;
        let limit = self.limits.max_timers;
        if current.saturating_add(amount) > limit {
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::Timers,
                current,
                requested: amount,
                limit,
            });
        }
        self.timers += amount;
        Ok(())
    }

    pub fn release_timers(&mut self, amount: usize) {
        self.timers = self.timers.saturating_sub(amount);
    }

    pub fn pending_states(&self) -> usize {
        self.pending_states
    }

    pub fn try_reserve_pending_states(&mut self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.pending_states;
        let limit = self.limits.max_pending_states;
        if current.saturating_add(amount) > limit {
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::PendingStates,
                current,
                requested: amount,
                limit,
            });
        }
        self.pending_states += amount;
        Ok(())
    }

    pub fn release_pending_states(&mut self, amount: usize) {
        self.pending_states = self.pending_states.saturating_sub(amount);
    }
}
