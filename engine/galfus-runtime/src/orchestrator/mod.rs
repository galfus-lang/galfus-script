#[cfg(test)]
mod tests;

pub(crate) mod cancellation;
pub(crate) mod effects;
pub(crate) mod external;
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

use pending::{LateCompletion, MAX_LATE_COMPLETIONS, PendingContinuation, PendingKey};
pub(crate) use startup::StartupPlan;

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
    next_request_id: u64,
    adapters: Option<Arc<std::sync::Mutex<galfus_contract::Adapters>>>,
    initialization_complete: Arc<AtomicBool>,
    shutting_down: bool,
    late_completions: VecDeque<LateCompletion>,
    root_thread_id: Option<crate::registry::ThreadId>,
    ready_futures: HashMap<u64, Result<galfus_contract::BoundaryValue, ExecutionFailure>>,
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
            adapters: None,
            initialization_complete: Arc::new(AtomicBool::new(true)),
            shutting_down: false,
            late_completions: VecDeque::new(),
            root_thread_id: None,
            ready_futures: HashMap::new(),
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

    pub(crate) fn set_adapters(
        &mut self,
        adapters: Option<Arc<std::sync::Mutex<galfus_contract::Adapters>>>,
    ) {
        self.assert_main_thread();
        self.adapters = adapters;
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
                .with_thread_id(thread_id.raw())
                .with_module_id(initialized_module_id.raw().into())
                .with_stack(execution_stack(&thread)),
            );
            self.kernel.cancel(thread_id);
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
                .with_thread_id(thread_id.raw())
                .with_module_id(initialized_module_id.raw().into()),
            );
            self.startup_plans.remove(&thread_id);
            self.kernel.cancel(thread_id);
            return;
        }
        self.kernel.enqueue_runnable(thread_id, thread);
    }

    pub(super) fn resume_or_fail(
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
            .resume(thread_id.raw(), &mut thread, continuation, value);
        match result {
            Ok(()) => self.kernel.enqueue_runnable(thread_id, thread),
            Err(error) => {
                self.failure = Some(with_execution_stack(
                    error.with_thread_id(thread_id.raw()),
                    execution_stack(&thread),
                ));
                self.kernel.cancel(thread_id);
            }
        }
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
            .resume(thread_id.raw(), &mut thread, continuation, value);
        match result {
            Ok(()) => self.kernel.enqueue_runnable_front(thread_id, thread),
            Err(error) => {
                self.failure = Some(with_execution_stack(
                    error.with_thread_id(thread_id.raw()),
                    execution_stack(&thread),
                ));
                self.kernel.cancel(thread_id);
            }
        }
    }

    fn record_late_completion(&mut self, thread_id: crate::registry::ThreadId, request_id: u64) {
        if self.late_completions.len() == MAX_LATE_COMPLETIONS {
            self.late_completions.pop_front();
        }
        self.late_completions.push_back(LateCompletion {
            thread_id,
            request_id,
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
            let id = match key {
                PendingKey::Request(id) | PendingKey::Future(id) => id,
            };
            self.record_late_completion(thread_id, id);
            return;
        };
        if pending.thread_id != thread_id {
            self.pending_continuations.insert(key, pending);
            let id = match key {
                PendingKey::Request(id) | PendingKey::Future(id) => id,
            };
            self.record_late_completion(thread_id, id);
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
                            .with_thread_id(thread_id.raw())
                            .with_module_id(pending.module_id.raw().into())
                            .with_stack(pending.stack.clone()),
                        );
                        self.kernel.cancel(thread_id);
                        return;
                    }
                };
                self.resume_or_fail_front(thread_id, thread, pending.continuation, value);
            }
            Err(error) => {
                let error = with_execution_stack(
                    with_pending_id(error)
                        .with_thread_id(thread_id.raw())
                        .with_module_id(pending.module_id.raw().into()),
                    pending.stack,
                );
                self.failure = Some(match thread.initializing_module() {
                    Some(initializing_module_id) => ExecutionFailure::new(
                        ExecutionFailureKind::InitializationFailure,
                        "module initializer asynchronous request failed",
                    )
                    .with_thread_id(thread_id.raw())
                    .with_module_id(initializing_module_id.raw().into())
                    .with_cause(error),
                    None => error,
                });
                self.kernel.cancel(thread_id);
            }
        }
    }

    pub(super) fn complete_future(
        &mut self,
        thread_id: crate::registry::ThreadId,
        future_id: u64,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) {
        let key = PendingKey::Future(future_id);
        if self.pending_continuations.contains_key(&key) {
            self.complete_pending(thread_id, key, result);
        } else {
            self.ready_futures.insert(future_id, result);
        }
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
                RuntimeEvent::ThreadSpawned { thread } => {
                    let id = self.kernel.spawn(thread, None);
                    let thread = self
                        .kernel
                        .take_thread(id)
                        .expect("spawned thread is registered");
                    self.kernel.enqueue_runnable(id, thread);
                }
                RuntimeEvent::Exited {
                    thread_id,
                    thread,
                    code,
                } => {
                    self.kernel.mark_exited(thread_id, thread, code);
                    let waiters = self.kernel.drain_waiters(thread_id);
                    for waiter in waiters {
                        self.complete_future(
                            waiter.waiter_id,
                            waiter.future_id,
                            Ok(BoundaryValue::I32(code)),
                        );
                    }
                }
                RuntimeEvent::Initialized {
                    thread_id,
                    thread,
                    module_id,
                } => self.advance_startup(thread_id, thread, module_id),
                RuntimeEvent::Failed { thread_id, error } => {
                    self.failure = Some(error.with_thread_id(thread_id.raw()));
                    self.cancel_pending_continuations(thread_id);
                    self.startup_plans.remove(&thread_id);
                    self.kernel.cancel(thread_id);
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
                } => self.complete_pending(thread_id, PendingKey::Future(future_id), result),
                RuntimeEvent::Tick { delta_ms } => {
                    self.kernel.tick(delta_ms);
                }
                RuntimeEvent::CancelExecution => {
                    self.shutting_down = true;
                    self.cancel_all_pending_continuations();
                    self.startup_plans.clear();
                    self.kernel.cancel_all();
                    self.failure = Some(galfus_contract::ExecutionFailure::new(
                        galfus_contract::ExecutionFailureKind::Cancelled,
                        "execution cancelled",
                    ));
                }
                RuntimeEvent::Syscall { thread_id, .. } if self.shutting_down => {
                    self.kernel.cancel(thread_id);
                }
                RuntimeEvent::Syscall {
                    thread_id,
                    thread,
                    effect,
                    continuation,
                } => {
                    self.handle_effect(thread_id, thread, effect, continuation);
                }
                RuntimeEvent::Yielded { thread_id, thread } => {
                    self.kernel.enqueue_runnable(thread_id, thread);
                }
                RuntimeEvent::CancelThread { thread_id } => {
                    self.cancel_pending_continuations(thread_id);
                    self.startup_plans.remove(&thread_id);
                    self.kernel.cancel(thread_id);
                }
            }
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
            return galfus_contract::ThreadResult::Completed(code);
        }

        galfus_contract::ThreadResult::Discarded
    }

    pub(crate) fn debug_states(
        &self,
    ) -> Vec<(crate::registry::ThreadId, crate::registry::ThreadState)> {
        self.kernel.debug_states()
    }
}
