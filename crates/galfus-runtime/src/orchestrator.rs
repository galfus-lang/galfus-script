#[cfg(test)]
mod tests;

use crate::event::{EventSink, RuntimeEvent};
use crate::kernel::VirtualKernel;
use crate::task::RuntimeTask;
use galfus_contract::{KernelDriver, KernelTask};
use galfus_vm::VirtualMachine;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, mpsc};
use std::thread::{self, ThreadId};

/// Proof that we are running on the main thread, since this cannot be sent across threads.
#[derive(Clone, Copy)]
pub struct MainThreadToken {
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

/// The Orchestrator is the heart of the execution lifecycle.
/// It owns the VirtualKernel and the Receiver for RuntimeEvents.
pub struct Orchestrator {
    kernel: VirtualKernel,
    receiver: mpsc::Receiver<RuntimeEvent>,
    sink: EventSink,
    driver: Option<Rc<dyn KernelDriver>>,
    vm: Option<Arc<VirtualMachine>>,
    main_thread_id: ThreadId,
    _not_send_sync: PhantomData<Rc<()>>,
    failure: Option<galfus_contract::ExecutionFailure>,
}

impl Orchestrator {
    pub fn new() -> Self {
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

    pub fn set_driver(&mut self, driver: Rc<dyn KernelDriver>) {
        self.assert_main_thread();
        self.driver = Some(driver);
    }

    pub fn set_vm(&mut self, vm: Arc<VirtualMachine>) {
        self.assert_main_thread();
        self.vm = Some(vm);
    }

    pub fn kernel_mut(&mut self, token: MainThreadToken) -> &mut VirtualKernel {
        token.assert_current();
        self.assert_main_thread();
        &mut self.kernel
    }

    pub fn kernel(&self, token: MainThreadToken) -> &VirtualKernel {
        token.assert_current();
        self.assert_main_thread();
        &self.kernel
    }

    pub fn sink(&self) -> EventSink {
        self.sink.clone()
    }

    /// Dispatches all currently runnable threads from the VirtualKernel to the driver.
    pub fn dispatch_runnables(&mut self, token: MainThreadToken) {
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
    pub fn process_events(&mut self, token: MainThreadToken) {
        token.assert_current();
        self.assert_main_thread();
        while let Ok(event) = self.receiver.try_recv() {
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
                RuntimeEvent::Failed { thread_id, error } => {
                    self.failure = Some(error);
                    self.kernel.cancel(thread_id);
                }
                RuntimeEvent::EffectCompleted {
                    thread_id,
                    continuation,
                    result,
                } => {
                    if let Some(mut thread) = self.kernel.take_thread(thread_id) {
                        let vm = self.vm.as_ref().unwrap();
                        match result {
                            Ok(value) => {
                                let value =
                                    crate::task::from_boundary_value(&mut thread.heap, value, vm);
                                if vm.resume(&mut thread, continuation, value).is_ok() {
                                    self.kernel.enqueue_runnable(thread_id, thread);
                                } else {
                                    self.kernel.cancel(thread_id);
                                }
                            }
                            Err(_) => {
                                self.kernel.cancel(thread_id);
                            }
                        }
                    }
                }
                RuntimeEvent::Syscall {
                    thread_id,
                    mut thread,
                    effect,
                    continuation,
                } => match effect {
                    galfus_vm::VmEffect::SendMsg { target, msg } => {
                        if target == 0 {
                            let host_val = crate::task::to_boundary_value(&thread.heap, msg);
                            if let Some(galfus_contract::BoundaryValue::Array {
                                mut values, ..
                            }) = host_val
                            {
                                if !values.is_empty() {
                                    let method_opt = match values.remove(0) {
                                        galfus_contract::BoundaryValue::Bytes(b) => {
                                            String::from_utf8(b).ok()
                                        }
                                        _ => None,
                                    };
                                    if let Some(method) = method_opt {
                                        let vm = self.vm.as_ref().unwrap();
                                        if let Some(providers) = vm.providers() {
                                            let mut p_lock = providers.lock().unwrap();
                                            if let Some(host) = p_lock.host_mut() {
                                                let injector = Arc::new(
                                                    crate::ExecutionHandle::for_continuation(
                                                        self.sink.clone(),
                                                        continuation.clone(),
                                                    ),
                                                );
                                                let tid = thread_id.raw() as usize;
                                                self.kernel.block(thread_id, thread, None);
                                                host.dispatch(tid, &method, &values, injector);
                                                continue;
                                            }
                                        }
                                    }
                                }
                            }
                            let _ = self.vm.as_ref().unwrap().resume(
                                &mut thread,
                                continuation,
                                galfus_vm::VmValue::Bool(false),
                            );
                            self.sink.send(RuntimeEvent::Failed {
                                thread_id,
                                error: galfus_contract::ExecutionFailure::new(
                                    galfus_contract::ExecutionFailureKind::MissingProvider,
                                    "HostProvider missing",
                                ),
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
                                        data: {
                                            let host_val =
                                                crate::task::to_boundary_value(&thread.heap, msg);
                                            if let Some(galfus_contract::BoundaryValue::Bytes(b)) =
                                                host_val
                                            {
                                                b
                                            } else {
                                                vec![]
                                            }
                                        },
                                    },
                                );
                                self.kernel.unblock(target_id);
                                success = true;
                            }
                        }
                        let _ = self.vm.as_ref().unwrap().resume(
                            &mut thread,
                            continuation,
                            galfus_vm::VmValue::Bool(success),
                        );
                        self.kernel.enqueue_runnable(thread_id, thread);
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
                        let _ = self.vm.as_ref().unwrap().resume(
                            &mut thread,
                            continuation,
                            galfus_vm::VmValue::Int64(new_id.raw() as i64),
                        );
                        self.kernel.enqueue_runnable(thread_id, thread);
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

                        let _ = self.vm.as_ref().unwrap().resume(
                            &mut thread,
                            continuation,
                            galfus_vm::VmValue::Bool(success),
                        );
                        self.kernel.enqueue_runnable(thread_id, thread);
                    }
                    galfus_vm::VmEffect::GetThread { key } => {
                        let val = crate::task::thread_key(&thread, key)
                            .and_then(|k| self.kernel.lookup_key(k.as_str()))
                            .map(|id| galfus_vm::VmValue::Int64(id.raw() as i64))
                            .unwrap_or(galfus_vm::VmValue::Int64(-1));
                        let _ = self
                            .vm
                            .as_ref()
                            .unwrap()
                            .resume(&mut thread, continuation, val);
                        self.kernel.enqueue_runnable(thread_id, thread);
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
                        let _ = self.vm.as_ref().unwrap().resume(
                            &mut thread,
                            continuation,
                            galfus_vm::VmValue::Bool(running),
                        );
                        self.kernel.enqueue_runnable(thread_id, thread);
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
                        let _ = self.vm.as_ref().unwrap().resume(
                            &mut thread,
                            continuation,
                            galfus_vm::VmValue::Bool(exited),
                        );
                        self.kernel.enqueue_runnable(thread_id, thread);
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
                        let _ = self
                            .vm
                            .as_ref()
                            .unwrap()
                            .resume(&mut thread, continuation, reason);
                        self.kernel.enqueue_runnable(thread_id, thread);
                    }
                    galfus_vm::VmEffect::Blocked => {
                        self.kernel.enqueue_runnable(thread_id, thread);
                    }
                },
                RuntimeEvent::CancelThread { thread_id } => {
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
