use std::collections::VecDeque;
use std::sync;
use std::sync::Mutex;
use std::thread;

use crate::event::{EventSequence, RuntimeEvent};
use galfus_contract::{
    ExecutionFailure, ExecutorStepResult, KernelDriver, KernelTask, LimitsMetadata, ThreadResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EventDeliveryError {
    #[error("execution event receiver is closed")]
    ReceiverClosed,
    #[error("execution event sequence is exhausted")]
    SequenceExhausted,
    #[error("execution event queue is unavailable")]
    QueueUnavailable,
}

pub trait RuntimeEventSink: Send + Sync {
    fn submit(&self, event: RuntimeEvent) -> Result<(), EventDeliveryError>;
}

pub trait ExecutionDriver: KernelDriver {
    fn event_sink(&self) -> std::sync::Arc<dyn RuntimeEventSink>;

    fn drain_events(&self) -> Vec<(EventSequence, RuntimeEvent)>;

    fn has_pending_events(&self) -> bool;

    fn configure_limits(&self, _limits: &LimitsMetadata) -> Result<(), EventDeliveryError> {
        Ok(())
    }
}

pub struct NativeEventBridge {
    sender: std::sync::mpsc::SyncSender<(EventSequence, RuntimeEvent)>,
    receiver: Mutex<std::sync::mpsc::Receiver<(EventSequence, RuntimeEvent)>>,
    next_sequence: Mutex<EventSequence>,
    pending: Mutex<usize>,
    capacity: usize,
    limit: Mutex<usize>,
}

impl Default for NativeEventBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeEventBridge {
    pub fn new() -> Self {
        Self::with_capacity(LimitsMetadata::default().max_event_queue)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, receiver) = std::sync::mpsc::sync_channel(capacity);
        Self {
            sender,
            receiver: Mutex::new(receiver),
            next_sequence: Mutex::new(EventSequence::FIRST),
            pending: Mutex::new(0),
            capacity,
            limit: Mutex::new(capacity),
        }
    }

    pub fn configure_limit(&self, limit: usize) -> Result<(), EventDeliveryError> {
        if limit > self.capacity {
            return Err(EventDeliveryError::QueueUnavailable);
        }
        *self
            .limit
            .lock()
            .map_err(|_| EventDeliveryError::QueueUnavailable)? = limit;
        Ok(())
    }

    pub fn drain(&self) -> Vec<(EventSequence, RuntimeEvent)> {
        let mut pending = self.pending.lock().unwrap();
        let events = self.receiver.lock().unwrap().try_iter().collect::<Vec<_>>();
        *pending = pending.saturating_sub(events.len());
        events
    }

    pub fn has_pending(&self) -> bool {
        *self.pending.lock().unwrap() != 0
    }
}

impl RuntimeEventSink for NativeEventBridge {
    fn submit(&self, event: RuntimeEvent) -> Result<(), EventDeliveryError> {
        let mut sequence = self
            .next_sequence
            .lock()
            .map_err(|_| EventDeliveryError::QueueUnavailable)?;
        let current = *sequence;
        let next = current
            .next()
            .ok_or(EventDeliveryError::SequenceExhausted)?;

        let mut pending = self
            .pending
            .lock()
            .map_err(|_| EventDeliveryError::QueueUnavailable)?;
        let limit = *self
            .limit
            .lock()
            .map_err(|_| EventDeliveryError::QueueUnavailable)?;
        if *pending >= limit {
            return Err(EventDeliveryError::QueueUnavailable);
        }

        self.sender
            .try_send((current, event))
            .map_err(|e| match e {
                std::sync::mpsc::TrySendError::Full(_) => EventDeliveryError::QueueUnavailable,
                std::sync::mpsc::TrySendError::Disconnected(_) => {
                    EventDeliveryError::QueueUnavailable
                }
            })?;

        *pending += 1;
        *sequence = next;
        Ok(())
    }
}

/// Runs Galfus tasks cooperatively on the calling host thread.
pub struct CooperativeDriver {
    queue: Mutex<VecDeque<KernelTask>>,
    events: std::sync::Arc<NativeEventBridge>,
    exit_result: sync::Mutex<Option<Result<i32, ExecutionFailure>>>,
    exit_callback: Mutex<Option<Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>>>,
}

impl CooperativeDriver {
    pub fn new() -> Self {
        Self::with_event_queue_capacity(LimitsMetadata::default().max_event_queue)
    }

    pub fn with_event_queue_capacity(event_queue_capacity: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            events: std::sync::Arc::new(NativeEventBridge::with_capacity(event_queue_capacity)),
            exit_result: sync::Mutex::new(None),
            exit_callback: Mutex::new(None),
        }
    }
}

impl KernelDriver for CooperativeDriver {
    fn dispatch(&self, task: KernelTask) {
        self.queue.lock().unwrap().push_back(task);
    }

    fn dispatch_front(&self, task: KernelTask) {
        self.queue.lock().unwrap().push_front(task);
    }

    fn on_exit(&self, callback: Box<dyn Fn(Result<i32, ExecutionFailure>) + Send + Sync>) {
        *self.exit_callback.lock().unwrap() = Some(callback);
    }

    fn run(&self) {
        let mut pending_timeout = None;
        loop {
            let task_entry = self.queue.lock().unwrap().pop_front();

            let Some(task_entry) = task_entry else {
                let Some(timeout) = pending_timeout.take() else {
                    break;
                };
                thread::sleep(timeout);
                continue;
            };

            let result = match task_entry {
                KernelTask::Main(task) => task.run(100),
                KernelTask::Any(task) => task.run(100),
            };

            match result {
                ThreadResult::Discarded => {}
                ThreadResult::Blocked { timeout } => {
                    pending_timeout = match (pending_timeout, timeout) {
                        (Some(current), Some(next)) => Some(current.min(next)),
                        (Some(current), None) => Some(current),
                        (None, next) => next,
                    };
                }
                ThreadResult::Completed(res) => {
                    let outcome = match res {
                        Ok(galfus_contract::BoundaryValue::I32(code)) => Ok(code),
                        Ok(_) => Ok(0),
                        Err(e) => Err(e),
                    };
                    *self.exit_result.lock().unwrap() = Some(outcome);
                }
            }
        }
        let outcome = self.exit_result.lock().unwrap().take().unwrap_or(Ok(0));
        if let Some(callback) = self.exit_callback.lock().unwrap().take() {
            callback(outcome);
        }
    }

    fn complete(&self, result: Result<i32, ExecutionFailure>) {
        *self.exit_result.lock().unwrap() = Some(result.clone());
        if let Some(callback) = self.exit_callback.lock().unwrap().take() {
            callback(result);
        }
    }

    fn step(&self) -> ExecutorStepResult {
        let task_entry = self.queue.lock().unwrap().pop_front();

        let Some(task_entry) = task_entry else {
            return ExecutorStepResult::Blocked { timeout: None };
        };

        let result = match task_entry {
            KernelTask::Main(task) => task.run(100),
            KernelTask::Any(task) => task.run(100),
        };

        match result {
            ThreadResult::Discarded => ExecutorStepResult::Running,
            ThreadResult::Blocked { timeout } => {
                let is_empty = self.queue.lock().unwrap().is_empty();
                if is_empty {
                    ExecutorStepResult::Blocked { timeout }
                } else {
                    ExecutorStepResult::Running
                }
            }
            ThreadResult::Completed(res) => {
                let outcome = match res {
                    Ok(galfus_contract::BoundaryValue::I32(c)) => Ok(c),
                    Ok(_) => Ok(0),
                    Err(e) => Err(e),
                };
                let code = match &outcome {
                    Ok(c) => *c,
                    Err(_) => 0,
                };
                *self.exit_result.lock().unwrap() = Some(outcome);
                let is_empty = self.queue.lock().unwrap().is_empty();
                if is_empty {
                    ExecutorStepResult::Completed(code)
                } else {
                    ExecutorStepResult::Running
                }
            }
        }
    }
}

impl ExecutionDriver for CooperativeDriver {
    fn event_sink(&self) -> std::sync::Arc<dyn RuntimeEventSink> {
        self.events.clone()
    }

    fn drain_events(&self) -> Vec<(EventSequence, RuntimeEvent)> {
        self.events.drain()
    }

    fn has_pending_events(&self) -> bool {
        self.events.has_pending()
    }

    fn configure_limits(&self, limits: &LimitsMetadata) -> Result<(), EventDeliveryError> {
        self.events.configure_limit(limits.max_event_queue)
    }
}

impl Default for CooperativeDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use galfus_contract::ExecutionFailureKind;
    use std::sync::Arc;

    #[test]
    fn complete_stores_and_calls_callback_with_error() {
        let driver = CooperativeDriver::new();
        let callback_called = Arc::new(Mutex::new(false));
        let callback_called_clone = Arc::clone(&callback_called);

        driver.on_exit(Box::new(move |result| {
            assert!(result.is_err());
            let error = result.unwrap_err();
            assert_eq!(error.kind, ExecutionFailureKind::VmPanic);
            *callback_called_clone.lock().unwrap() = true;
        }));

        driver.complete(Err(ExecutionFailure::new(
            ExecutionFailureKind::VmPanic,
            "test error",
        )));

        assert!(*callback_called.lock().unwrap());

        let stored = driver.exit_result.lock().unwrap().clone();
        assert!(stored.is_some());
        let stored_result = stored.unwrap();
        assert!(stored_result.is_err());
        assert_eq!(
            stored_result.unwrap_err().kind,
            ExecutionFailureKind::VmPanic
        );
    }

    #[test]
    fn event_bridge_enforces_the_configured_limit_and_recovers_after_drain() {
        let bridge = NativeEventBridge::with_capacity(2);
        bridge.configure_limit(1).unwrap();

        bridge.submit(RuntimeEvent::Tick { delta_ms: 1 }).unwrap();
        assert!(matches!(
            bridge.submit(RuntimeEvent::Tick { delta_ms: 1 }),
            Err(EventDeliveryError::QueueUnavailable)
        ));

        assert_eq!(bridge.drain().len(), 1);
        bridge.submit(RuntimeEvent::Tick { delta_ms: 1 }).unwrap();
    }
}
