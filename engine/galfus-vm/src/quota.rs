use galfus_contract::{ExecutionFailureKind, LimitsMetadata, ResourceLimitKind};

#[derive(Debug, Clone)]
pub struct GlobalQuota {
    limits: LimitsMetadata,
    threads: usize,
    futures: usize,
    pending_requests: usize,
    event_queue: usize,
    kernel_tasks: usize,
    runnable_threads: usize,
    external_handles: usize,
    timers: usize,
    pending_states: usize,
}

impl GlobalQuota {
    pub fn new(limits: LimitsMetadata) -> Self {
        Self {
            limits,
            threads: 0,
            futures: 0,
            pending_requests: 0,
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

    pub fn try_reserve_pending_requests(
        &mut self,
        amount: usize,
    ) -> Result<(), ExecutionFailureKind> {
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

    pub fn try_reserve_runnable_threads(
        &mut self,
        amount: usize,
    ) -> Result<(), ExecutionFailureKind> {
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

    pub fn try_reserve_external_handles(
        &mut self,
        amount: usize,
    ) -> Result<(), ExecutionFailureKind> {
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

    pub fn try_reserve_pending_states(
        &mut self,
        amount: usize,
    ) -> Result<(), ExecutionFailureKind> {
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

use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
pub struct ThreadQuota {
    limits: LimitsMetadata,
    heap_objects: AtomicUsize,
    heap_bytes: AtomicUsize,
    call_depth: AtomicUsize,
    mailbox_messages: AtomicUsize,
    mailbox_bytes: AtomicUsize,
}

impl ThreadQuota {
    pub fn new(limits: LimitsMetadata) -> Self {
        Self {
            limits,
            heap_objects: AtomicUsize::new(0),
            heap_bytes: AtomicUsize::new(0),
            call_depth: AtomicUsize::new(0),
            mailbox_messages: AtomicUsize::new(0),
            mailbox_bytes: AtomicUsize::new(0),
        }
    }

    pub fn limits(&self) -> &LimitsMetadata {
        &self.limits
    }

    pub fn heap_objects(&self) -> usize {
        self.heap_objects.load(Ordering::Relaxed)
    }

    pub fn try_reserve_heap_objects(&self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.heap_objects.fetch_add(amount, Ordering::Relaxed);
        let limit = self.limits.max_heap_objects;
        if current.saturating_add(amount) > limit {
            self.heap_objects.fetch_sub(amount, Ordering::Relaxed);
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::HeapObjects,
                current,
                requested: amount,
                limit,
            });
        }
        Ok(())
    }

    pub fn release_heap_objects(&self, amount: usize) {
        self.heap_objects.fetch_sub(amount, Ordering::Relaxed);
    }

    pub fn try_reserve_heap_bytes(&self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.heap_bytes.fetch_add(amount, Ordering::Relaxed);
        let limit = self.limits.max_heap_bytes;
        if current.saturating_add(amount) > limit {
            self.heap_bytes.fetch_sub(amount, Ordering::Relaxed);
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::HeapBytes,
                current,
                requested: amount,
                limit,
            });
        }
        Ok(())
    }

    pub fn try_reserve_heap(
        &self,
        objects: usize,
        bytes: usize,
    ) -> Result<(), ExecutionFailureKind> {
        let obj_current = self.heap_objects.fetch_add(objects, Ordering::Relaxed);
        let obj_limit = self.limits.max_heap_objects;
        if obj_current.saturating_add(objects) > obj_limit {
            self.heap_objects.fetch_sub(objects, Ordering::Relaxed);
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::HeapObjects,
                current: obj_current,
                requested: objects,
                limit: obj_limit,
            });
        }

        let byte_current = self.heap_bytes.fetch_add(bytes, Ordering::Relaxed);
        let byte_limit = self.limits.max_heap_bytes;
        if byte_current.saturating_add(bytes) > byte_limit {
            self.heap_bytes.fetch_sub(bytes, Ordering::Relaxed);
            self.heap_objects.fetch_sub(objects, Ordering::Relaxed);
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::HeapBytes,
                current: byte_current,
                requested: bytes,
                limit: byte_limit,
            });
        }

        Ok(())
    }

    pub fn release_heap(&self, objects: usize, bytes: usize) {
        self.heap_objects.fetch_sub(objects, Ordering::Relaxed);
        self.heap_bytes.fetch_sub(bytes, Ordering::Relaxed);
    }

    pub fn release_heap_bytes(&self, amount: usize) {
        self.heap_bytes.fetch_sub(amount, Ordering::Relaxed);
    }

    pub fn try_reserve_call_depth(&self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.call_depth.fetch_add(amount, Ordering::Relaxed);
        let limit = self.limits.max_call_depth;
        if current.saturating_add(amount) > limit {
            self.call_depth.fetch_sub(amount, Ordering::Relaxed);
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::CallDepth,
                current,
                requested: amount,
                limit,
            });
        }
        Ok(())
    }

    pub fn release_call_depth(&self, amount: usize) {
        self.call_depth.fetch_sub(amount, Ordering::Relaxed);
    }

    pub fn try_reserve_mailbox_messages(&self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.mailbox_messages.fetch_add(amount, Ordering::Relaxed);
        let limit = self.limits.max_mailbox_messages;
        if current.saturating_add(amount) > limit {
            self.mailbox_messages.fetch_sub(amount, Ordering::Relaxed);
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::MailboxMessages,
                current,
                requested: amount,
                limit,
            });
        }
        Ok(())
    }

    pub fn release_mailbox_messages(&self, amount: usize) {
        self.mailbox_messages.fetch_sub(amount, Ordering::Relaxed);
    }

    pub fn try_reserve_mailbox_bytes(&self, amount: usize) -> Result<(), ExecutionFailureKind> {
        let current = self.mailbox_bytes.fetch_add(amount, Ordering::Relaxed);
        let limit = self.limits.max_mailbox_bytes;
        if current.saturating_add(amount) > limit {
            self.mailbox_bytes.fetch_sub(amount, Ordering::Relaxed);
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::MailboxBytes,
                current,
                requested: amount,
                limit,
            });
        }
        Ok(())
    }

    pub fn try_reserve_mailbox(
        &self,
        messages: usize,
        bytes: usize,
    ) -> Result<(), ExecutionFailureKind> {
        let msg_current = self.mailbox_messages.fetch_add(messages, Ordering::Relaxed);
        let msg_limit = self.limits.max_mailbox_messages;
        if msg_current.saturating_add(messages) > msg_limit {
            self.mailbox_messages.fetch_sub(messages, Ordering::Relaxed);
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::MailboxMessages,
                current: msg_current,
                requested: messages,
                limit: msg_limit,
            });
        }
        let byte_current = self.mailbox_bytes.fetch_add(bytes, Ordering::Relaxed);
        let byte_limit = self.limits.max_mailbox_bytes;
        if byte_current.saturating_add(bytes) > byte_limit {
            self.mailbox_bytes.fetch_sub(bytes, Ordering::Relaxed);
            self.mailbox_messages.fetch_sub(messages, Ordering::Relaxed);
            return Err(ExecutionFailureKind::ResourceLimitExceeded {
                resource: ResourceLimitKind::MailboxBytes,
                current: byte_current,
                requested: bytes,
                limit: byte_limit,
            });
        }
        Ok(())
    }

    pub fn release_mailbox_bytes(&self, amount: usize) {
        self.mailbox_bytes.fetch_sub(amount, Ordering::Relaxed);
    }

    pub fn release_mailbox(&self, messages: usize, bytes: usize) {
        self.mailbox_messages.fetch_sub(messages, Ordering::Relaxed);
        self.mailbox_bytes.fetch_sub(bytes, Ordering::Relaxed);
    }
}
