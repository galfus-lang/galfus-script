#[cfg(test)]
mod tests;

pub(crate) mod adapter;
pub(crate) mod cancellation;
pub(crate) mod effects;
pub(crate) mod future_registry;
pub(crate) mod pending;
pub(crate) mod startup;

use crate::event::{EventSink, RuntimeEvent};
use crate::kernel::VirtualKernel;
use crate::task::{RuntimeTask, execution_stack, with_execution_stack};
use galfus_contract::{
    BoundaryValue, ExecutionFailure, ExecutionFailureKind, KernelDriver, KernelTask,
};
use galfus_vm::VirtualMachine;
use std::collections::{HashMap, VecDeque};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::{self, ThreadId};

use future_registry::FutureRegistry;
use pending::{
    LateCompletion, MAX_LATE_COMPLETIONS, PendingContinuation, PendingKey, PendingOperation,
};
pub(crate) use startup::StartupPlan;

fn collect_adapter_handles(
    value: &BoundaryValue,
    handles: &mut Vec<(galfus_core::OpaqueTypeId, galfus_core::HandleId)>,
) {
    match value {
        BoundaryValue::Array { values, .. } | BoundaryValue::Tuple(values) => {
            for value in values {
                collect_adapter_handles(value, handles);
            }
        }
        BoundaryValue::Choice {
            payload: Some(payload),
            ..
        } => collect_adapter_handles(payload, handles),
        BoundaryValue::Handle { type_id, id, .. } => handles.push((type_id.clone(), *id)),
        _ => {}
    }
}

fn stamp_adapter_handles(
    value: &mut BoundaryValue,
    proxy_module: Option<&str>,
    binding_id: Option<galfus_core::BindingId>,
) -> bool {
    match value {
        BoundaryValue::Array { values, .. } | BoundaryValue::Tuple(values) => values
            .iter_mut()
            .all(|value| stamp_adapter_handles(value, proxy_module, binding_id)),
        BoundaryValue::Choice {
            payload: Some(payload),
            ..
        } => stamp_adapter_handles(payload, proxy_module, binding_id),
        BoundaryValue::Handle {
            type_id,
            binding_id: handle_binding_id,
            ..
        } => {
            let valid = type_id.proxy_module()
                == proxy_module.unwrap_or_default().trim_end_matches(".gfp")
                && handle_binding_id.is_none()
                && binding_id.is_some();
            if valid {
                *handle_binding_id = binding_id;
            }
            valid
        }
        _ => true,
    }
}

/// Proof that orchestration runs on its bound host main thread.
#[derive(Clone, Copy)]
pub(crate) struct MainThreadToken {
    thread_id: ThreadId,
    _marker: PhantomData<*mut ()>,
}

impl MainThreadToken {
    fn new() -> Self {
        Self {
            thread_id: thread::current().id(),
            _marker: PhantomData,
        }
    }

    fn assert_current(self) {
        assert_eq!(
            self.thread_id,
            thread::current().id(),
            "main-thread token used from another thread"
        );
    }
}

pub(crate) struct Orchestrator {
    kernel: VirtualKernel,
    receiver: mpsc::Receiver<(u64, RuntimeEvent)>,
    sink: EventSink,
    driver: Option<Rc<dyn KernelDriver>>,
    vm: Option<Arc<VirtualMachine>>,
    main_thread_id: ThreadId,
    _not_send_sync: PhantomData<Rc<()>>,
    pub(crate) failure: Option<galfus_contract::ExecutionFailure>,
    pending_continuations: HashMap<PendingKey, PendingContinuation>,
    startup_plans: HashMap<crate::registry::ThreadId, StartupPlan>,
    next_request_id: u32,
    next_future_id: u32,
    next_coordinator_id: u32,
    adapter_bindings: Option<Arc<std::sync::Mutex<galfus_contract::AdapterBindings>>>,
    initialization_complete: Arc<AtomicBool>,
    shutting_down: bool,
    late_completions: VecDeque<LateCompletion>,
    root_thread_id: Option<crate::registry::ThreadId>,
    future_workers: HashMap<crate::registry::ThreadId, (crate::registry::ThreadId, galfus_core::FutureId)>,
    thread_exit_waits: HashMap<crate::registry::ThreadId, Vec<(crate::registry::ThreadId, galfus_core::FutureId)>>,
    mailbox_future_waits: HashMap<crate::registry::ThreadId, Vec<MailboxFutureWait>>,
    virtual_time_ms: u64,
    pub(crate) future_registry: FutureRegistry,
    aggregate_coordinators: HashMap<u32, AggregateCoordinator>,
    aggregate_registration: Option<(u32, usize)>,
}

#[derive(Clone, Copy)]
pub(crate) enum AggregateMode {
    All,
    Race,
}

pub(crate) struct AggregateCoordinator {
    pub(crate) mode: AggregateMode,
    pub(crate) future_ids: Vec<galfus_core::FutureId>,
    pub(crate) pending: PendingContinuation,
    pub(crate) results: Vec<Option<Result<BoundaryValue, ExecutionFailure>>>,
    pub(crate) winner: Option<Result<BoundaryValue, ExecutionFailure>>,
    pub(crate) armed: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct MailboxFutureWait {
    owner_thread_id: crate::registry::ThreadId,
    future_id: galfus_core::FutureId,
    sender_id: Option<crate::registry::ThreadId>,
    deadline_ms: Option<u64>,
}

impl Orchestrator {
    pub(crate) fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            kernel: VirtualKernel::new(),
            receiver,
            sink: EventSink::new(sender),
            driver: None,
            vm: None,
            main_thread_id: thread::current().id(),
            _not_send_sync: PhantomData,
            failure: None,
            pending_continuations: HashMap::new(),
            startup_plans: HashMap::new(),
            next_request_id: 1,
            next_future_id: 1,
            next_coordinator_id: 1,
            adapter_bindings: None,
            initialization_complete: Arc::new(AtomicBool::new(true)),
            shutting_down: false,
            late_completions: VecDeque::new(),
            root_thread_id: None,
            future_workers: HashMap::new(),
            thread_exit_waits: HashMap::new(),
            mailbox_future_waits: HashMap::new(),
            virtual_time_ms: 0,
            future_registry: FutureRegistry::new(),
            aggregate_coordinators: HashMap::new(),
            aggregate_registration: None,
        }
    }

    pub(crate) fn set_root_thread(&mut self, thread_id: crate::registry::ThreadId) {
        self.assert_main_thread();
        self.root_thread_id = Some(thread_id);
    }

    pub(crate) fn main_thread_token(&self) -> MainThreadToken {
        self.assert_main_thread();
        MainThreadToken::new()
    }

    fn assert_main_thread(&self) {
        assert_eq!(
            self.main_thread_id,
            thread::current().id(),
            "orchestrator accessed from a non-main thread"
        );
    }

    pub(crate) fn set_driver(&mut self, driver: Rc<dyn KernelDriver>) {
        self.assert_main_thread();
        self.driver = Some(driver);
    }

    pub(crate) fn set_vm(&mut self, vm: Arc<VirtualMachine>) {
        self.assert_main_thread();
        self.vm = Some(vm);
    }

    pub(crate) fn set_adapter_bindings(
        &mut self,
        bindings: Option<Arc<std::sync::Mutex<galfus_contract::AdapterBindings>>>,
    ) {
        self.assert_main_thread();
        self.adapter_bindings = bindings;
    }

    pub(crate) fn kernel_mut(&mut self, token: MainThreadToken) -> &mut VirtualKernel {
        token.assert_current();
        self.assert_main_thread();
        &mut self.kernel
    }

    #[cfg(test)]
    pub(crate) fn kernel(&self, token: MainThreadToken) -> &VirtualKernel {
        token.assert_current();
        self.assert_main_thread();
        &self.kernel
    }

    pub(crate) fn sink(&self) -> EventSink {
        self.sink.clone()
    }

    pub(crate) fn initialization_complete(&self) -> Arc<AtomicBool> {
        self.initialization_complete.clone()
    }

    pub(crate) fn set_startup_plan(
        &mut self,
        thread_id: crate::registry::ThreadId,
        plan: StartupPlan,
    ) {
        self.assert_main_thread();
        self.initialization_complete.store(false, Ordering::Release);
        self.startup_plans.insert(thread_id, plan);
    }

    fn advance_startup(
        &mut self,
        thread_id: crate::registry::ThreadId,
        mut thread: galfus_vm::thread::VmThreadState,
        initialized_module_id: galfus_core::ModuleId,
    ) {
        thread.mark_module_initialized(initialized_module_id);
        let Some(mut plan) = self.startup_plans.remove(&thread_id) else {
            self.failure = Some(
                galfus_contract::ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::InitializationFailure,
                    "module initializer completed without a startup plan",
                )
                .with_thread_id(thread_id)
                .with_module_id(initialized_module_id.raw().into())
                .with_stack(execution_stack(&thread)),
            );
            self.cancel_and_teardown_thread(thread_id);
            return;
        };

        let (module_id, function, args) = match plan.initializers.pop_front() {
            Some((module_id, function)) => {
                thread.begin_module_initialization(module_id);
                self.startup_plans.insert(thread_id, plan);
                (module_id, function, vec![])
            }
            None => {
                self.initialization_complete.store(true, Ordering::Release);
                (plan.entry_module_id, plan.entry_func, vec![plan.entry_args])
            }
        };
        let vm = self.vm.as_ref().expect("VM is configured before execution");
        if let Err(error) = vm.prepare_function(&mut thread, module_id, function, args) {
            self.failure = Some(
                galfus_contract::ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::InitializationFailure,
                    error.to_string(),
                )
                .with_thread_id(thread_id)
                .with_module_id(initialized_module_id.raw().into()),
            );
            self.startup_plans.remove(&thread_id);
            self.cancel_and_teardown_thread(thread_id);
            return;
        }
        self.kernel.enqueue_runnable(thread_id, thread);
    }

    pub(super) fn resume_or_fail_front(
        &mut self,
        thread_id: crate::registry::ThreadId,
        mut thread: galfus_vm::thread::VmThreadState,
        continuation: galfus_vm::Continuation,
        value: galfus_vm::VmValue,
    ) {
        let result = self
            .vm
            .as_ref()
            .expect("VM is configured before execution")
            .resume(thread_id, &mut thread, continuation, value);
        match result {
            Ok(()) => self.kernel.enqueue_runnable_front(thread_id, thread),
            Err(error) => {
                self.failure = Some(with_execution_stack(
                    error.with_thread_id(thread_id),
                    execution_stack(&thread),
                ));
                self.cancel_and_teardown_thread(thread_id);
            }
        }
    }

    fn record_late_completion(&mut self, thread_id: crate::registry::ThreadId, key: crate::orchestrator::pending::PendingKey) {
        if self.late_completions.len() == MAX_LATE_COMPLETIONS {
            self.late_completions.pop_front();
        }
        self.late_completions.push_back(LateCompletion {
            thread_id,
            key,
        });
    }

    #[cfg(test)]
    fn late_completion_count(&self) -> usize {
        self.late_completions.len()
    }

    fn complete_pending(
        &mut self,
        thread_id: crate::registry::ThreadId,
        key: PendingKey,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) {
        let Some(pending) = self.pending_continuations.remove(&key) else {
            self.record_late_completion(thread_id, key);
            return;
        };
        if pending.thread_id != thread_id {
            self.pending_continuations.insert(key, pending);
            self.record_late_completion(thread_id, key);
            return;
        }
        self.resume_pending(thread_id, pending, result, key);
    }

    fn resume_pending(
        &mut self,
        thread_id: crate::registry::ThreadId,
        pending: PendingContinuation,
        result: Result<BoundaryValue, ExecutionFailure>,
        key: PendingKey,
    ) {
        if let PendingOperation::AggregateMember {
            coordinator_id,
            index,
        } = pending.operation
        {
            self.complete_aggregate_member(coordinator_id, index, result);
            return;
        }
        self.kernel.unblock(thread_id);
        let Some(mut thread) = self.kernel.take_thread(thread_id) else {
            return;
        };
        let vm = self
            .vm
            .as_ref()
            .expect("VM is configured before execution")
            .clone();
        let with_pending_id = |failure: ExecutionFailure| match key {
            PendingKey::Request(request_id) => failure.with_request_id(request_id),
            PendingKey::Future(future_id) => failure.with_future_id(future_id),
            PendingKey::Coordinator(_) => failure,
        };
        match result {
            Ok(value) => {
                let module = &vm
                    .graph
                    .get(pending.module_id)
                    .expect("asynchronous call module is loaded")
                    .module;
                let value = match crate::task::encode_into_thread_heap(
                    &mut thread.heap,
                    value,
                    pending.return_type,
                    pending.module_id,
                    module,
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.failure = Some(
                            with_pending_id(ExecutionFailure::new(
                                ExecutionFailureKind::BoundaryCodecFailure,
                                format!("invalid asynchronous result: {error:?}"),
                            ))
                            .with_thread_id(thread_id)
                            .with_module_id(pending.module_id.raw().into())
                            .with_stack(pending.stack.clone()),
                        );
                        self.cancel_and_teardown_thread(thread_id);
                        return;
                    }
                };
                self.resume_or_fail_front(thread_id, thread, pending.continuation, value);
            }
            Err(error) => {
                let error = with_execution_stack(
                    with_pending_id(error)
                        .with_thread_id(thread_id)
                        .with_module_id(pending.module_id.raw().into()),
                    pending.stack,
                );
                self.failure = Some(match thread.initializing_module() {
                    Some(initializing_module_id) => ExecutionFailure::new(
                        ExecutionFailureKind::InitializationFailure,
                        "module initializer asynchronous request failed",
                    )
                    .with_thread_id(thread_id)
                    .with_module_id(initializing_module_id.raw().into())
                    .with_cause(error),
                    None => error,
                });
                self.cancel_and_teardown_thread(thread_id);
            }
        }
    }

    pub(super) fn complete_future(
        &mut self,
        thread_id: crate::registry::ThreadId,
        future_id: galfus_core::FutureId,
        mut result: Result<BoundaryValue, ExecutionFailure>,
    ) {
        let adapter_proxy_module = self
            .future_registry
            .adapter_proxy_module(thread_id, future_id);

        let binding_id = adapter_proxy_module.as_deref().and_then(|proxy_module| {
            self.adapter_bindings
                .as_ref()
                .and_then(|bindings| bindings.lock().ok()?.binding_id(proxy_module))
        });
        if result.as_mut().is_ok_and(|value| {
            !stamp_adapter_handles(value, adapter_proxy_module.as_deref(), binding_id)
        }) {
            result = Err(ExecutionFailure::new(
                ExecutionFailureKind::BoundaryCodecFailure,
                "adapter returned a handle for a different binding or opaque type namespace",
            ));
        }

        if let (Some((payload_module_id, payload_type)), Ok(value)) = (
            self.future_registry.payload_schema(thread_id, future_id),
            &result,
        ) {
            let module = &self
                .vm
                .as_ref()
                .expect("VM is configured before execution")
                .graph
                .get(payload_module_id)
                .expect("future payload module is loaded")
                .module;
            let mut payload_heap = galfus_vm::thread::PrivateHeap::new();
            if let Err(error) = crate::task::encode_into_thread_heap(
                &mut payload_heap,
                value.clone(),
                payload_type,
                payload_module_id,
                module,
            ) {
                self.failure = Some(
                    ExecutionFailure::new(
                        ExecutionFailureKind::BoundaryCodecFailure,
                        format!("invalid future worker result: {error:?}"),
                    )
                    .with_thread_id(thread_id)
                    .with_future_id(future_id)
                    .with_module_id(payload_module_id.raw().into()),
                );
                self.cancel_and_teardown_thread(thread_id);
                return;
            }
        }
        let waiters = match self
            .future_registry
            .complete(thread_id, future_id, result.clone())
        {
            Ok(waiters) => waiters,
            Err(error) => {
                if error.kind == ExecutionFailureKind::DuplicateCompletion {
                    self.record_late_completion(thread_id, PendingKey::Future(future_id));
                    return;
                }
                self.failure = Some(error);
                self.cancel_and_teardown_thread(thread_id);
                return;
            }
        };
        if let (Some(proxy_module), Ok(value)) = (adapter_proxy_module, &result)
            && !self.register_adapter_handles(&proxy_module, value)
        {
            self.failure = Some(
                ExecutionFailure::new(
                    ExecutionFailureKind::BoundaryCodecFailure,
                    "adapter returned an external handle without a unique bound owner",
                )
                .with_thread_id(thread_id)
                .with_future_id(future_id),
            );
            self.cancel_and_teardown_thread(thread_id);
            return;
        }
        for waiter in waiters {
            let waiter_thread_id = waiter.continuation.thread_id;
            self.resume_pending(
                waiter_thread_id,
                waiter.continuation,
                result.clone(),
                PendingKey::Future(future_id),
            );
        }
    }

    fn register_adapter_handles(&mut self, proxy_module: &str, value: &BoundaryValue) -> bool {
        let mut handles = Vec::new();
        collect_adapter_handles(value, &mut handles);
        if handles.is_empty() {
            return true;
        }
        let Some(bindings) = &self.adapter_bindings else {
            return false;
        };
        let mut bindings = bindings.lock().unwrap();
        let Some(binding_id) = bindings.binding_id(proxy_module) else {
            return false;
        };
        bindings.register_handles(binding_id, &handles).is_ok()
    }

    pub(super) fn register_thread_exit_future(
        &mut self,
        target_thread_id: crate::registry::ThreadId,
        owner_thread_id: crate::registry::ThreadId,
        future_id: galfus_core::FutureId,
    ) {
        self.thread_exit_waits
            .entry(target_thread_id)
            .or_default()
            .push((owner_thread_id, future_id));
    }

    pub(super) fn register_mailbox_future_wait(
        &mut self,
        target_thread_id: crate::registry::ThreadId,
        owner_thread_id: crate::registry::ThreadId,
        future_id: galfus_core::FutureId,
        sender_id: Option<crate::registry::ThreadId>,
        timeout_ms: Option<u64>,
    ) {
        self.mailbox_future_waits
            .entry(target_thread_id)
            .or_default()
            .push(MailboxFutureWait {
                owner_thread_id,
                future_id,
                sender_id,
                deadline_ms: timeout_ms.map(|timeout| self.virtual_time_ms.saturating_add(timeout)),
            });
    }

    pub(super) fn complete_mailbox_future_waits(
        &mut self,
        target_thread_id: crate::registry::ThreadId,
    ) {
        loop {
            let Some(wait) = self
                .mailbox_future_waits
                .get(&target_thread_id)
                .and_then(|waits| waits.first().copied())
            else {
                return;
            };
            let message = self
                .kernel
                .get_mailbox(target_thread_id)
                .and_then(|mailbox| {
                    let mut mailbox = mailbox.lock().unwrap();
                    let index = wait.sender_id.map_or_else(
                        || (!mailbox.is_empty()).then_some(0),
                        |sender_id| {
                            mailbox
                                .iter()
                                .position(|message| message.sender_id == sender_id)
                        },
                    )?;
                    mailbox.remove(index)
                });
            let Some(message) = message else {
                return;
            };
            let remove_entry = {
                let waits = self
                    .mailbox_future_waits
                    .get_mut(&target_thread_id)
                    .expect("mailbox wait is registered");
                waits.remove(0);
                waits.is_empty()
            };
            if remove_entry {
                self.mailbox_future_waits.remove(&target_thread_id);
            }
            self.complete_future(
                wait.owner_thread_id,
                wait.future_id,
                Ok(BoundaryValue::Bytes(message.data)),
            );
        }
    }

    pub(super) fn expire_mailbox_future_waits(&mut self, delta_ms: u64) {
        self.virtual_time_ms = self.virtual_time_ms.saturating_add(delta_ms);
        let mut expired = Vec::new();
        self.mailbox_future_waits.retain(|_, waits| {
            let mut index = 0;
            while index < waits.len() {
                if waits[index]
                    .deadline_ms
                    .is_some_and(|deadline| deadline <= self.virtual_time_ms)
                {
                    expired.push(waits.remove(index));
                } else {
                    index += 1;
                }
            }
            !waits.is_empty()
        });
        for wait in expired {
            self.complete_future(
                wait.owner_thread_id,
                wait.future_id,
                Ok(BoundaryValue::Null),
            );
        }
    }

    pub(super) fn remove_mailbox_future_wait(
        &mut self,
        owner_thread_id: crate::registry::ThreadId,
        future_id: galfus_core::FutureId,
    ) {
        self.mailbox_future_waits.retain(|_, waits| {
            waits.retain(|wait| {
                wait.owner_thread_id != owner_thread_id || wait.future_id != future_id
            });
            !waits.is_empty()
        });
    }

    fn complete_aggregate_member(
        &mut self,
        coordinator_id: u32,
        index: usize,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) {
        let Some(coordinator) = self.aggregate_coordinators.get_mut(&coordinator_id) else {
            return;
        };
        if index >= coordinator.results.len() || coordinator.results[index].is_some() {
            return;
        }
        coordinator.results[index] = Some(result.clone());
        if matches!(coordinator.mode, AggregateMode::Race) && coordinator.winner.is_none() {
            coordinator.winner = Some(result);
        }
        if !coordinator.armed {
            return;
        }
        self.finish_aggregate_if_ready(coordinator_id);
    }

    pub(super) fn finish_aggregate_if_ready(&mut self, coordinator_id: u32) {
        let Some(coordinator) = self.aggregate_coordinators.get(&coordinator_id) else {
            return;
        };
        let result = match coordinator.mode {
            AggregateMode::All if coordinator.results.iter().all(Option::is_some) => {
                let values = coordinator
                    .results
                    .iter()
                    .map(|result| result.as_ref().expect("all results are present").clone())
                    .collect::<Result<Vec<_>, _>>();
                values.map(BoundaryValue::Tuple)
            }
            AggregateMode::Race => match coordinator.winner.clone() {
                Some(result) => result,
                None => return,
            },
            AggregateMode::All => return,
        };
        let Some(coordinator) = self.aggregate_coordinators.remove(&coordinator_id) else {
            return;
        };
        if matches!(coordinator.mode, AggregateMode::Race) {
            for future_id in coordinator.future_ids {
                let disposition = self
                    .future_registry
                    .discard_for_race(coordinator.pending.thread_id, future_id);
                if let Ok(future_registry::DiscardDisposition::Running(activation)) = disposition {
                    self.cancel_future_activation(
                        coordinator.pending.thread_id,
                        future_id,
                        activation,
                    );
                }
            }
        }
        self.resume_pending(
            coordinator.pending.thread_id,
            coordinator.pending,
            result,
            PendingKey::Coordinator(coordinator_id),
        );
    }

    /// Dispatches all currently runnable threads from the VirtualKernel to the driver.
    pub(crate) fn dispatch_runnables(&mut self, token: MainThreadToken) {
        token.assert_current();
        self.assert_main_thread();
        while let Some((thread_id, is_front)) = self.kernel.next_runnable_detailed() {
            if let Some(thread) = self.kernel.take_thread(thread_id) {
                self.kernel.mark_running(thread_id);

                let task = Box::new(RuntimeTask::new(
                    thread_id,
                    thread,
                    self.vm.as_ref().unwrap().clone(),
                    self.sink.clone(),
                    self.future_workers.get(&thread_id).copied(),
                ));

                let kernel_task = KernelTask::Any(task);
                if is_front {
                    self.driver.as_ref().unwrap().dispatch_front(kernel_task);
                } else {
                    self.driver.as_ref().unwrap().dispatch(kernel_task);
                }
            }
        }
    }

    /// Processes all pending events in the queue without blocking.
    pub(crate) fn process_events(&mut self, token: MainThreadToken) {
        token.assert_current();
        self.assert_main_thread();
        while let Ok((_event_id, event)) = self.receiver.try_recv() {
            self.sink.mark_received();
            match event {
                RuntimeEvent::ThreadSpawned { mut thread } => {
                    self.flush_thread_handle_drops(&mut thread);
                    let id = self.kernel.spawn(thread, None).expect("failed to spawn thread");
                    let thread = self
                        .kernel
                        .take_thread(id)
                        .expect("spawned thread is registered");
                    self.kernel.enqueue_runnable(id, thread);
                }
                RuntimeEvent::Exited {
                    thread_id,
                    mut thread,
                    result,
                } => {
                    self.teardown_thread_handles(&mut thread);
                    self.kernel.mark_exited(thread_id, thread, result.clone());
                    let waiters = self
                        .thread_exit_waits
                        .remove(&thread_id)
                        .unwrap_or_default();
                    for (owner_thread_id, future_id) in waiters {
                        self.complete_future(owner_thread_id, future_id, result.clone());
                    }
                }
                RuntimeEvent::Initialized {
                    thread_id,
                    mut thread,
                    module_id,
                } => {
                    self.flush_thread_handle_drops(&mut thread);
                    self.advance_startup(thread_id, thread, module_id)
                }
                RuntimeEvent::Failed { thread_id, error } => {
                    self.failure = Some(error.with_thread_id(thread_id));
                    self.cancel_pending_continuations(thread_id);
                    self.startup_plans.remove(&thread_id);
                    self.cancel_and_teardown_thread(thread_id);
                }
                RuntimeEvent::EffectCompleted {
                    thread_id,
                    request_id,
                    result,
                } => self.complete_pending(thread_id, PendingKey::Request(request_id), result),
                RuntimeEvent::FutureCompleted {
                    thread_id,
                    future_id,
                    result,
                } => self.complete_future(thread_id, future_id, result),
                RuntimeEvent::FutureWorkerCompleted {
                    worker_thread_id,
                    owner_thread_id,
                    future_id,
                    mut thread,
                    result,
                } => {
                    self.future_workers.remove(&worker_thread_id);
                    self.teardown_thread_handles(&mut thread);
                    self.kernel
                        .mark_exited(worker_thread_id, thread, result.clone());
                    self.complete_future(owner_thread_id, future_id, result);
                }
                RuntimeEvent::Tick { delta_ms } => {
                    self.kernel.tick(delta_ms);
                    self.expire_mailbox_future_waits(delta_ms);
                }
                RuntimeEvent::CancelExecution => {
                    self.shutting_down = true;
                    self.cancel_all_pending_continuations();
                    self.cancel_all_futures();
                    self.startup_plans.clear();
                    self.cancel_and_teardown_all_threads();
                    self.failure = Some(galfus_contract::ExecutionFailure::new(
                        galfus_contract::ExecutionFailureKind::Cancelled,
                        "execution cancelled",
                    ));
                }
                RuntimeEvent::Syscall { thread_id, .. } if self.shutting_down => {
                    self.cancel_and_teardown_thread(thread_id);
                }
                RuntimeEvent::Syscall {
                    thread_id,
                    mut thread,
                    effect,
                    continuation,
                } => {
                    self.flush_thread_handle_drops(&mut thread);
                    self.handle_effect(thread_id, thread, effect, continuation);
                }
                RuntimeEvent::Yielded {
                    thread_id,
                    mut thread,
                } => {
                    self.flush_thread_handle_drops(&mut thread);
                    self.kernel.enqueue_runnable(thread_id, thread);
                }
                RuntimeEvent::CancelThread { thread_id } => {
                    self.cancel_pending_continuations(thread_id);
                    self.cancel_thread_futures(thread_id);
                    self.startup_plans.remove(&thread_id);
                    self.cancel_and_teardown_thread(thread_id);
                }
            }
        }
    }

    fn flush_thread_handle_drops(&mut self, thread: &mut galfus_vm::thread::VmThreadState) {
        let handles = std::mem::take(&mut thread.pending_adapter_handle_drops);
        if handles.is_empty() {
            return;
        }
        if let Some(bindings) = &self.adapter_bindings {
            let mut bindings = bindings.lock().unwrap();
            for (binding_id, type_id, id) in handles {
                bindings.release_handle(binding_id, &type_id, id);
            }
        }
    }

    fn teardown_thread_handles(&mut self, thread: &mut galfus_vm::thread::VmThreadState) {
        let handles = thread.extract_all_adapter_handles();
        if handles.is_empty() {
            return;
        }
        if let Some(bindings) = &self.adapter_bindings {
            let mut bindings = bindings.lock().unwrap();
            for (binding_id, type_id, id) in handles {
                bindings.release_handle(binding_id, &type_id, id);
            }
        }
    }

    pub(crate) fn cancel_and_teardown_thread(&mut self, thread_id: crate::registry::ThreadId) {
        if let Some(mut thread) = self.kernel.take_thread(thread_id) {
            self.teardown_thread_handles(&mut thread);
        }
        self.kernel.cancel(thread_id);
    }

    pub(crate) fn cancel_and_teardown_all_threads(&mut self) {
        let thread_ids = self
            .kernel
            .debug_states()
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        for thread_id in thread_ids {
            self.cancel_and_teardown_thread(thread_id);
        }
    }
}

impl Orchestrator {
    pub(crate) fn step(&mut self, _budget: usize) -> galfus_contract::ThreadResult {
        let token = self.main_thread_token();
        self.process_events(token);
        self.dispatch_runnables(token);

        if self.failure.is_some() {
            return galfus_contract::ThreadResult::Discarded;
        }

        if self.kernel.active_count() == 0 {
            let code = self
                .root_thread_id
                .and_then(|id| self.kernel.get_exit_code(id))
                .unwrap_or(0);
            return galfus_contract::ThreadResult::Completed(Ok(
                galfus_contract::BoundaryValue::I32(code),
            ));
        }

        galfus_contract::ThreadResult::Discarded
    }

    pub(crate) fn debug_states(
        &self,
    ) -> Vec<(crate::registry::ThreadId, crate::registry::ThreadState)> {
        self.kernel.debug_states()
    }
}
