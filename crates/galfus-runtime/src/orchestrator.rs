#[cfg(test)]
mod tests;

use crate::event::{EventSink, RuntimeEvent};
use crate::kernel::VirtualKernel;
use crate::task::RuntimeTask;
use galfus_contract::{
    BoundaryValue, ExecutionFailure, ExecutionFailureKind, KernelDriver, KernelTask,
    MessageInjector, RunnableTask, TaskAffinity, ThreadResult,
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

struct PendingContinuation {
    thread_id: crate::registry::ThreadId,
    continuation: galfus_vm::Continuation,
    module_id: galfus_core::ModuleId,
    return_type: galfus_bytecode::instruction::TypeIdx,
    request_id: u64,
}

struct ProviderDispatchTask {
    providers: Arc<std::sync::Mutex<galfus_contract::Providers>>,
    thread_id: usize,
    request_id: u64,
    name: String,
    args: Vec<BoundaryValue>,
    injector: Arc<dyn MessageInjector>,
}

impl ProviderDispatchTask {
    fn into_kernel_task(self, affinity: TaskAffinity) -> KernelTask {
        match affinity {
            TaskAffinity::Main => KernelTask::Main(Box::new(self)),
            TaskAffinity::Any => KernelTask::Any(Box::new(self)),
        }
    }
}

impl RunnableTask for ProviderDispatchTask {
    fn run(self: Box<Self>, _budget: usize) -> ThreadResult {
        let mut providers = self.providers.lock().unwrap();
        let Some(host) = providers.host_mut() else {
            self.injector.inject_system_response(
                self.thread_id,
                self.request_id,
                Err(ExecutionFailure::new(
                    ExecutionFailureKind::MissingProvider,
                    "HostProvider missing while dispatching request",
                )
                .with_request_id(self.request_id)),
            );
            return ThreadResult::Completed(0);
        };
        host.dispatch(
            self.thread_id,
            self.request_id,
            self.name.as_str(),
            self.args.as_slice(),
            self.injector.clone(),
        );
        ThreadResult::Completed(0)
    }

    fn into_any_thread(self: Box<Self>) -> Option<Box<dyn RunnableTask + Send>> {
        Some(self)
    }
}

pub(crate) struct StartupPlan {
    pub(crate) initializers:
        VecDeque<(galfus_core::ModuleId, galfus_bytecode::instruction::FuncIdx)>,
    pub(crate) entry_module_id: galfus_core::ModuleId,
    pub(crate) entry_func: galfus_bytecode::instruction::FuncIdx,
    pub(crate) entry_args: galfus_vm::VmValue,
}

/// Proof that orchestration runs on its bound host main thread.
///
/// On single-threaded WASM this is the creation event-loop context; native
/// targets additionally validate the bound operating-system thread identity.
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

/// The runtime-internal owner of the VirtualKernel and runtime-event receiver.
pub(crate) struct Orchestrator {
    kernel: VirtualKernel,
    receiver: mpsc::Receiver<(u64, RuntimeEvent)>,
    sink: EventSink,
    driver: Option<Rc<dyn KernelDriver>>,
    vm: Option<Arc<VirtualMachine>>,
    main_thread_id: ThreadId,
    _not_send_sync: PhantomData<Rc<()>>,
    failure: Option<galfus_contract::ExecutionFailure>,
    pending_continuations: HashMap<u64, PendingContinuation>,
    startup_plans: HashMap<crate::registry::ThreadId, StartupPlan>,
    next_request_id: u64,
    initialization_complete: Arc<AtomicBool>,
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
            initialization_complete: Arc::new(AtomicBool::new(true)),
        }
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
        mut thread: galfus_vm::thread::VirtualThread,
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
                .with_module_id(initialized_module_id.raw().into()),
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

    fn resume_or_fail(
        &mut self,
        thread_id: crate::registry::ThreadId,
        mut thread: galfus_vm::thread::VirtualThread,
        continuation: galfus_vm::Continuation,
        value: galfus_vm::VmValue,
    ) {
        let result = self
            .vm
            .as_ref()
            .expect("VM is configured before execution")
            .resume(&mut thread, continuation, value);
        match result {
            Ok(()) => self.kernel.enqueue_runnable(thread_id, thread),
            Err(error) => {
                self.failure = Some(error.with_thread_id(thread_id.raw()));
                self.kernel.cancel(thread_id);
            }
        }
    }

    fn cancel_pending_continuation(&mut self, thread_id: crate::registry::ThreadId) {
        let request_id = self
            .pending_continuations
            .iter()
            .find_map(|(&request_id, pending)| {
                (pending.thread_id == thread_id).then_some(request_id)
            });
        let Some(request_id) = request_id else {
            return;
        };
        let Some(pending) = self.pending_continuations.remove(&request_id) else {
            return;
        };
        let Some(vm) = self.vm.as_ref() else {
            return;
        };
        let Some(providers) = vm.providers() else {
            return;
        };
        let mut providers = providers.lock().unwrap();
        if let Some(host) = providers.host_mut() {
            host.cancel(thread_id.raw() as usize, pending.request_id);
        }
    }

    fn cancel_all_pending_continuations(&mut self) {
        let thread_ids = self
            .pending_continuations
            .values()
            .map(|pending| pending.thread_id)
            .collect::<Vec<_>>();
        for thread_id in thread_ids {
            self.cancel_pending_continuation(thread_id);
        }
    }

    /// Dispatches all currently runnable threads from the VirtualKernel to the driver.
    pub(crate) fn dispatch_runnables(&mut self, token: MainThreadToken) {
        token.assert_current();
        self.assert_main_thread();
        while let Some(thread_id) = self.kernel.next_runnable() {
            if let Some(mut thread) = self.kernel.take_thread(thread_id) {
                let _ = thread.mark_running();
                self.kernel.mark_running(thread_id);

                let task = Box::new(RuntimeTask::new(
                    thread_id,
                    thread,
                    self.vm.as_ref().unwrap().clone(),
                    self.sink.clone(),
                ));

                self.driver
                    .as_ref()
                    .unwrap()
                    .dispatch(KernelTask::Any(task));
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
                    let id = self.kernel.spawn(thread);
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
                }
                RuntimeEvent::Initialized {
                    thread_id,
                    thread,
                    module_id,
                } => self.advance_startup(thread_id, thread, module_id),
                RuntimeEvent::Failed { thread_id, error } => {
                    self.failure = Some(error.with_thread_id(thread_id.raw()));
                    self.cancel_pending_continuation(thread_id);
                    self.startup_plans.remove(&thread_id);
                    self.kernel.cancel(thread_id);
                }
                RuntimeEvent::EffectCompleted {
                    thread_id,
                    request_id,
                    result,
                } => {
                    let Some(pending) = self.pending_continuations.remove(&request_id) else {
                        continue;
                    };
                    if pending.thread_id != thread_id {
                        self.pending_continuations.insert(request_id, pending);
                        continue;
                    }
                    if let Some(mut thread) = self.kernel.take_thread(thread_id) {
                        let vm = self
                            .vm
                            .as_ref()
                            .expect("VM is configured before execution")
                            .clone();
                        match result {
                            Ok(value) => {
                                let module = &vm
                                    .graph
                                    .get(pending.module_id)
                                    .expect("provider call module is loaded")
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
                                        self.failure = Some(galfus_contract::ExecutionFailure::new(
                                            galfus_contract::ExecutionFailureKind::BoundaryCodecFailure,
                                            format!("invalid provider result: {error:?}"),
                                        ).with_thread_id(thread_id.raw()).with_request_id(pending.request_id).with_module_id(pending.module_id.raw().into()));
                                        self.kernel.cancel(thread_id);
                                        continue;
                                    }
                                };
                                self.resume_or_fail(thread_id, thread, pending.continuation, value);
                            }
                            Err(error) => {
                                let error = error
                                    .with_thread_id(thread_id.raw())
                                    .with_request_id(pending.request_id)
                                    .with_module_id(pending.module_id.raw().into());
                                self.failure = Some(match thread.initializing_module() {
                                    Some(initializing_module_id) => ExecutionFailure::new(
                                        ExecutionFailureKind::InitializationFailure,
                                        "module initializer provider request failed",
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
                }
                RuntimeEvent::Tick { delta_ms } => {
                    self.kernel.tick(delta_ms);
                }
                RuntimeEvent::CancelExecution => {
                    self.cancel_all_pending_continuations();
                    self.startup_plans.clear();
                    self.kernel.cancel_all();
                    self.failure = Some(galfus_contract::ExecutionFailure::new(
                        galfus_contract::ExecutionFailureKind::Cancelled,
                        "execution cancelled",
                    ));
                }
                RuntimeEvent::Syscall {
                    thread_id,
                    thread,
                    effect,
                    continuation,
                } => match effect {
                    galfus_vm::VmEffect::ProviderCall {
                        module_id,
                        name,
                        args,
                        arg_types,
                        return_type,
                    } => {
                        let vm = self.vm.as_ref().expect("VM is configured before execution");
                        let module = &vm
                            .graph
                            .get(module_id)
                            .expect("provider call module is loaded")
                            .module;
                        let args = args
                            .into_iter()
                            .zip(arg_types)
                            .map(|(value, ty)| {
                                crate::task::decode_from_thread_heap(
                                    &thread.heap,
                                    value,
                                    ty,
                                    module,
                                )
                            })
                            .collect::<Result<Vec<_>, _>>();
                        let args = match args {
                            Ok(args) => args,
                            Err(error) => {
                                self.failure = Some(
                                    galfus_contract::ExecutionFailure::new(
                                        galfus_contract::ExecutionFailureKind::BoundaryCodecFailure,
                                        format!("invalid provider argument: {error:?}"),
                                    )
                                    .with_thread_id(thread_id.raw())
                                    .with_module_id(module_id.raw().into()),
                                );
                                self.kernel.cancel(thread_id);
                                continue;
                            }
                        };
                        let Some(providers) = vm.providers() else {
                            self.failure = Some(
                                ExecutionFailure::new(
                                    ExecutionFailureKind::MissingProvider,
                                    "HostProvider missing",
                                )
                                .with_thread_id(thread_id.raw())
                                .with_module_id(module_id.raw().into()),
                            );
                            self.kernel.cancel(thread_id);
                            continue;
                        };
                        let affinity = {
                            let mut providers = providers.lock().unwrap();
                            let Some(host) = providers.host_mut() else {
                                self.failure = Some(
                                    ExecutionFailure::new(
                                        ExecutionFailureKind::MissingProvider,
                                        "HostProvider missing",
                                    )
                                    .with_thread_id(thread_id.raw())
                                    .with_module_id(module_id.raw().into()),
                                );
                                self.kernel.cancel(thread_id);
                                continue;
                            };
                            host.affinity(name.as_str())
                        };
                        let request_id = self.next_request_id;
                        self.next_request_id += 1;
                        self.pending_continuations.insert(
                            request_id,
                            PendingContinuation {
                                thread_id,
                                continuation,
                                module_id,
                                return_type,
                                request_id,
                            },
                        );
                        let injector = Arc::new(crate::ExecutionHandle::new(self.sink.clone()));
                        self.kernel.block(thread_id, thread, None);
                        let task = ProviderDispatchTask {
                            providers,
                            thread_id: thread_id.raw() as usize,
                            request_id,
                            name,
                            args,
                            injector,
                        };
                        self.driver
                            .as_ref()
                            .expect("driver is configured before execution")
                            .dispatch(task.into_kernel_task(affinity));
                        continue;
                    }
                    galfus_vm::VmEffect::SendMsg { target, bytes } => {
                        if target == 0 {
                            self.sink.send(RuntimeEvent::Failed {
                                thread_id,
                                error: galfus_contract::ExecutionFailure::new(
                                    galfus_contract::ExecutionFailureKind::InvalidBytecode,
                                    "host calls must use the ProviderCall VM effect",
                                )
                                .with_thread_id(thread_id.raw()),
                            });
                            continue;
                        }
                        let target_id = crate::registry::ThreadId::from_raw(target);
                        let mut success = false;
                        if let Some(target_id) = target_id {
                            if let Some(mailbox) = self.kernel.get_mailbox(target_id) {
                                mailbox.lock().unwrap().push_back(
                                    galfus_vm::thread::MailboxMessage {
                                        sender_id: thread_id.raw(),
                                        data: bytes,
                                    },
                                );
                                self.kernel.unblock(target_id);
                                success = true;
                            }
                        }
                        self.resume_or_fail(
                            thread_id,
                            thread,
                            continuation,
                            galfus_vm::VmValue::Bool(success),
                        );
                    }
                    galfus_vm::VmEffect::AdapterCall { .. } => {
                        self.failure = Some(
                            ExecutionFailure::new(
                                ExecutionFailureKind::MissingAdapter,
                                "adapter calls require an adapter registry",
                            )
                            .with_thread_id(thread_id.raw()),
                        );
                        self.kernel.cancel(thread_id);
                    }
                    galfus_vm::VmEffect::TimerWait { delay_ms } => {
                        self.kernel.block(thread_id, thread, Some(delay_ms));
                    }
                    galfus_vm::VmEffect::FutureWait { .. } => {
                        self.failure = Some(
                            ExecutionFailure::new(
                                ExecutionFailureKind::InternalRuntimeFailure,
                                "future waits require a future registry",
                            )
                            .with_thread_id(thread_id.raw()),
                        );
                        self.kernel.cancel(thread_id);
                    }
                    galfus_vm::VmEffect::ReceiveFilter {
                        sender_id: _,
                        timeout,
                    } => {
                        if let Some(ms) = timeout {
                            self.kernel.block(thread_id, thread, Some(ms));
                        } else {
                            self.kernel.block(thread_id, thread, None);
                        }
                    }
                    galfus_vm::VmEffect::CreateThread { func, key } => {
                        let mut new_thread = galfus_vm::thread::VirtualThread::new();
                        new_thread.entry_func = Some(func.clone());
                        if let Some(k) = crate::task::thread_key(&thread, key) {
                            new_thread.key = Some(k);
                        }
                        let new_id = self.kernel.spawn(new_thread);
                        self.resume_or_fail(
                            thread_id,
                            thread,
                            continuation,
                            galfus_vm::VmValue::Int64(new_id.raw() as i64),
                        );
                    }
                    galfus_vm::VmEffect::StartThread {
                        thread_id: target_id,
                        arg,
                    } => {
                        let target_id = crate::registry::ThreadId::from_raw(target_id);
                        let mut success = false;

                        if let Some(target_id) = target_id {
                            let target_thread = self
                                .kernel
                                .take_thread(target_id)
                                .filter(|t| t.state == galfus_vm::thread::ThreadState::Created);
                            if let Some(mut target_thread) = target_thread {
                                let prepared = match target_thread.entry_func.clone() {
                                    Some(galfus_vm::VmValue::Function {
                                        module_id,
                                        func_idx,
                                    }) => self.vm.as_ref().unwrap().prepare_function(
                                        &mut target_thread,
                                        module_id,
                                        func_idx,
                                        vec![arg],
                                    ),
                                    _ => Err(galfus_vm::VmPanic {
                                        error: galfus_vm::VmError::FunctionOutOfBounds {
                                            index: galfus_bytecode::instruction::FuncIdx(0),
                                        },
                                        stack_trace: vec![],
                                    }),
                                };
                                if prepared.is_ok() {
                                    self.kernel.enqueue_runnable(target_id, target_thread);
                                    success = true;
                                } else {
                                    self.kernel.park_running(target_id, target_thread);
                                }
                            }
                        }

                        self.resume_or_fail(
                            thread_id,
                            thread,
                            continuation,
                            galfus_vm::VmValue::Bool(success),
                        );
                    }
                    galfus_vm::VmEffect::GetThread { key } => {
                        let val = crate::task::thread_key(&thread, key)
                            .and_then(|k| self.kernel.lookup_key(k.as_str()))
                            .map(|id| galfus_vm::VmValue::Int64(id.raw() as i64))
                            .unwrap_or(galfus_vm::VmValue::Int64(-1));
                        self.resume_or_fail(thread_id, thread, continuation, val);
                    }
                    galfus_vm::VmEffect::ThreadIsRunning {
                        thread_id: target_id,
                    } => {
                        let running = crate::registry::ThreadId::from_raw(target_id)
                            .and_then(|target_id| {
                                self.kernel.take_thread(target_id).map(|t| {
                                    let state = t.state;
                                    self.kernel.park_running(target_id, t);
                                    state
                                })
                            })
                            .is_some_and(|state| state.is_running());
                        self.resume_or_fail(
                            thread_id,
                            thread,
                            continuation,
                            galfus_vm::VmValue::Bool(running),
                        );
                    }
                    galfus_vm::VmEffect::ThreadIsExited {
                        thread_id: target_id,
                    } => {
                        let exited = crate::registry::ThreadId::from_raw(target_id)
                            .and_then(|target_id| {
                                self.kernel.take_thread(target_id).map(|t| {
                                    let state = t.state;
                                    self.kernel.park_running(target_id, t);
                                    state
                                })
                            })
                            .is_some_and(|state| state.is_exited());
                        self.resume_or_fail(
                            thread_id,
                            thread,
                            continuation,
                            galfus_vm::VmValue::Bool(exited),
                        );
                    }
                    galfus_vm::VmEffect::ThreadExitReason {
                        thread_id: target_id,
                    } => {
                        let reason = crate::registry::ThreadId::from_raw(target_id)
                            .and_then(|target_id| {
                                self.kernel.take_thread(target_id).map(|t| {
                                    let state = t.state;
                                    self.kernel.park_running(target_id, t);
                                    state
                                })
                            })
                            .and_then(|state| state.exit_reason())
                            .map(galfus_vm::VmValue::Int32)
                            .unwrap_or(galfus_vm::VmValue::Null);
                        self.resume_or_fail(thread_id, thread, continuation, reason);
                    }
                    galfus_vm::VmEffect::Blocked => {
                        self.kernel.enqueue_runnable(thread_id, thread);
                    }
                },
                RuntimeEvent::CancelThread { thread_id } => {
                    self.cancel_pending_continuation(thread_id);
                    self.startup_plans.remove(&thread_id);
                    self.kernel.cancel(thread_id);
                }
            }
        }
    }
}

impl galfus_contract::RunnableTask for Orchestrator {
    fn run(mut self: Box<Self>, _budget: usize) -> galfus_contract::ThreadResult {
        let token = self.main_thread_token();
        self.process_events(token);
        self.dispatch_runnables(token);

        if let Some(failure) = self.failure.take() {
            return galfus_contract::ThreadResult::Failed(failure);
        }

        if self.kernel.active_count() == 0 {
            let code = self
                .kernel
                .get_exit_code(crate::registry::ThreadId::from_raw(1).unwrap())
                .unwrap_or(0);
            return galfus_contract::ThreadResult::Completed(code);
        }

        galfus_contract::ThreadResult::Yielded(self)
    }
}
