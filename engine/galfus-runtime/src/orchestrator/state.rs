use super::*;

use crate::task::execution_stack;
use galfus_contract::ExecutionFailure;

impl Orchestrator {
    #[cfg(test)]
    pub(crate) fn test_new() -> Self {
        Self::new(std::sync::Arc::new(std::sync::Mutex::new(
            galfus_vm::quota::GlobalQuota::new(galfus_contract::LimitsMetadata::default()),
        )))
    }

    pub(crate) fn new(
        quota: std::sync::Arc<std::sync::Mutex<galfus_vm::quota::GlobalQuota>>,
    ) -> Self {
        Self {
            kernel: VirtualKernel::new(),
            driver: None,
            event_sink: None,
            pending_events: BTreeMap::new(),
            next_event_sequence: EventSequence::FIRST,
            active_event_sequence: None,
            pending_aggregate_finishes: BTreeSet::new(),
            vm: None,
            _not_send_sync: PhantomData,
            failure: None,
            pending_continuations: HashMap::new(),
            startup_plans: HashMap::new(),
            request_id_manager: galfus_core::id_manager::LocalIdManager::new(1),
            request_generations: HashMap::new(),
            future_id_manager: galfus_core::id_manager::LocalIdManager::new(1),
            future_generations: HashMap::new(),
            coordinator_id_manager: galfus_core::id_manager::LocalIdManager::new(1),
            adapter_bindings: None,
            initialization_complete: Arc::new(AtomicBool::new(true)),
            shutting_down: false,
            shutdown_report: None,
            cancellation_report: CancellationReport::default(),
            completion_metrics: CompletionMetrics::default(),
            #[cfg(feature = "metrics")]
            future_metrics: FutureMetrics::default(),
            late_completions: VecDeque::new(),
            root_thread_id: None,
            future_workers: HashMap::new(),
            thread_exit_waits: HashMap::new(),
            mailbox_future_waits: HashMap::new(),
            mailbox_future_wait_targets: HashMap::new(),
            mailbox_deadlines: BTreeSet::new(),
            mailbox_wait_sequence: 0,
            timer_future_waits: BTreeSet::new(),
            virtual_time_ms: 0,
            future_registry: FutureRegistry::new(),
            aggregate_coordinators: HashMap::new(),
            aggregate_registration: None,
            quota,
        }
    }

    pub(crate) fn set_root_thread(&mut self, thread_id: crate::registry::ThreadId) {
        self.root_thread_id = Some(thread_id);
    }

    pub(crate) fn set_driver(&mut self, driver: Rc<dyn ExecutionDriver>) {
        self.driver = Some(driver);
    }

    pub(crate) fn set_event_sink(&mut self, sink: Arc<dyn RuntimeEventSink>) {
        self.event_sink = Some(sink);
    }

    pub(crate) fn set_vm(&mut self, vm: Arc<VirtualMachine>) {
        self.vm = Some(vm);
    }

    pub(crate) fn set_adapter_bindings(
        &mut self,
        bindings: Option<Arc<std::sync::Mutex<galfus_contract::AdapterBindings>>>,
    ) {
        self.adapter_bindings = bindings;
    }

    pub(crate) fn kernel_mut(&mut self) -> &mut VirtualKernel {
        &mut self.kernel
    }

    #[cfg(test)]
    pub(crate) fn kernel(&self) -> &VirtualKernel {
        &self.kernel
    }

    pub(crate) fn initialization_complete(&self) -> Arc<AtomicBool> {
        self.initialization_complete.clone()
    }

    pub(crate) fn cancellation_report(&self) -> &CancellationReport {
        &self.cancellation_report
    }

    pub(crate) fn completion_metrics(&self) -> &CompletionMetrics {
        &self.completion_metrics
    }

    #[cfg(feature = "metrics")]
    pub(crate) fn future_metrics(&self) -> &FutureMetrics {
        &self.future_metrics
    }

    pub(crate) fn set_startup_plan(
        &mut self,
        thread_id: crate::registry::ThreadId,
        plan: StartupPlan,
    ) {
        self.initialization_complete.store(false, Ordering::Release);
        self.startup_plans.insert(thread_id, plan);
    }

    pub(super) fn advance_startup(
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
        if let Err(e) = self.kernel.enqueue_runnable(thread_id, thread) {
            self.failure = Some(
                ExecutionFailure::new(e, "runnable threads limit exceeded")
                    .with_thread_id(thread_id),
            );
            self.cancel_and_teardown_thread(thread_id);
        }
    }
}
