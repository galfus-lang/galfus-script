use super::*;
use crate::event::EventSequence;
use crate::orchestrator::adapter::ProviderDispatchTask;
use crate::orchestrator::pending::PendingOperation;
use galfus_bytecode::instruction::{Reg, TypeIdx};
use galfus_contract::{
    BoundaryValue, ExecutionFailure, ExecutionFailureKind, HostProvider, KernelTask,
    MessageInjector, Providers, RunnableTask, TaskAffinity, ThreadResult,
};
use galfus_core::{CoordinatorId, FutureId, ModuleId, ThreadId};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct RecordingProvider(Arc<AtomicBool>);

impl HostProvider for RecordingProvider {
    fn descriptor(&self) -> galfus_contract::ProviderDescriptor {
        galfus_contract::ProviderDescriptor::default()
    }

    fn dispatch(
        &mut self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
        _name: &str,
        _args: &[BoundaryValue],
        _injector: Arc<dyn MessageInjector>,
    ) {
        self.0.store(true, Ordering::Release);
    }
}

struct NoopInjector;

impl MessageInjector for NoopInjector {
    fn inject_system_response(
        &self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
        _result: Result<BoundaryValue, galfus_contract::ExecutionFailure>,
    ) -> Result<(), galfus_contract::MessageInjectionError> {
        Ok(())
    }
}

struct FailureInjector(Arc<Mutex<Vec<ExecutionFailure>>>);

impl MessageInjector for FailureInjector {
    fn inject_system_response(
        &self,
        _thread_id: galfus_core::ThreadId,
        _request_lease: galfus_core::RequestLease,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) -> Result<(), galfus_contract::MessageInjectionError> {
        if let Err(failure) = result {
            self.0.lock().unwrap().push(failure);
        }
        Ok(())
    }
}

pub(super) fn provider_dispatch_task(called: Arc<AtomicBool>) -> ProviderDispatchTask {
    ProviderDispatchTask {
        providers: Arc::new(Mutex::new(
            Providers::new().with_host("io", Box::new(RecordingProvider(called))),
        )),
        thread_id: galfus_core::ThreadId::new(1),
        request_lease: galfus_core::RequestLease::new(galfus_core::RequestId::new(1), 1),
        alias: "io".to_string(),
        name: "operation".to_string(),
        args: vec![],
        injector: Arc::new(NoopInjector),
        active: Arc::new(AtomicBool::new(true)),
    }
}

#[test]
pub(super) fn provider_dispatch_tasks_use_the_declared_driver_lane() {
    let called = Arc::new(AtomicBool::new(false));
    let task = provider_dispatch_task(called.clone());
    let KernelTask::Main(task) = task.into_kernel_task(TaskAffinity::Main) else {
        panic!("main-affine provider must receive a main task");
    };
    assert!(matches!(Box::new(task).run(1), ThreadResult::Discarded));
    assert!(called.load(Ordering::Acquire));

    let task = provider_dispatch_task(Arc::new(AtomicBool::new(false)));
    assert!(matches!(
        task.into_kernel_task(TaskAffinity::Any),
        KernelTask::Any(_)
    ));
}

#[test]
pub(super) fn provider_dispatch_reports_a_poisoned_registry_without_panicking() {
    let providers = Arc::new(Mutex::new(Providers::default()));
    let poisoned = providers.clone();
    assert!(
        std::thread::spawn(move || {
            let _guard = poisoned.lock().unwrap();
            panic!("poison provider registry");
        })
        .join()
        .is_err()
    );

    let failures = Arc::new(Mutex::new(Vec::new()));
    let task = ProviderDispatchTask {
        providers,
        thread_id: galfus_core::ThreadId::new(1),
        request_lease: galfus_core::RequestLease::new(galfus_core::RequestId::new(1), 1),
        alias: "io".to_string(),
        name: "operation".to_string(),
        args: vec![],
        injector: Arc::new(FailureInjector(failures.clone())),
        active: Arc::new(AtomicBool::new(true)),
    };

    assert!(matches!(Box::new(task).run(1), ThreadResult::Discarded));
    assert_eq!(
        failures.lock().unwrap().as_slice(),
        [ExecutionFailure::new(
            ExecutionFailureKind::InternalRuntimeFailure,
            "provider registry lock is poisoned",
        )]
    );
}

#[test]
pub(super) fn cancelled_provider_dispatch_tasks_do_not_start_adapter_work() {
    let called = Arc::new(AtomicBool::new(false));
    let task = provider_dispatch_task(called.clone());
    task.active.store(false, Ordering::Release);

    assert!(matches!(Box::new(task).run(1), ThreadResult::Discarded));
    assert!(!called.load(Ordering::Acquire));
}

#[test]
pub(super) fn spawned_event_is_registered_and_queued_by_the_execution_owner() {
    let mut orchestrator = Orchestrator::test_new();
    orchestrator.submit_event(RuntimeEvent::ThreadSpawned {
        thread: galfus_vm::thread::VmThreadState::test_new(),
    });

    orchestrator.process_events();

    assert_eq!(orchestrator.kernel().active_count(), 1);
    assert_eq!(orchestrator.kernel().runnable_count(), 1);
}

#[test]
pub(super) fn race_winner_uses_event_sequence_then_member_index() {
    let mut orchestrator = Orchestrator::test_new();
    let coordinator_id = CoordinatorId::new(1);
    let thread_id = ThreadId::new(1);
    let module_id = ModuleId::new(1);
    orchestrator.aggregate_coordinators.insert(
        coordinator_id,
        AggregateCoordinator {
            mode: AggregateMode::Race,
            future_ids: vec![FutureId::new(1), FutureId::new(2), FutureId::new(3)],
            pending: PendingContinuation {
                thread_id,
                continuation: galfus_vm::Continuation::for_provider(Reg(0), module_id, TypeIdx(0)),
                module_id,
                return_type: TypeIdx(0),
                stack: vec![],
                operation: PendingOperation::Future,
                active: Arc::new(AtomicBool::new(true)),
            },
            results: None,
            remaining_results: 0,
            winner: None,
            armed: false,
        },
    );

    orchestrator.active_event_sequence = Some(EventSequence(3));
    orchestrator.complete_aggregate_member(coordinator_id, 2, Ok(BoundaryValue::I32(3)));
    orchestrator.active_event_sequence = Some(EventSequence(1));
    orchestrator.complete_aggregate_member(coordinator_id, 1, Ok(BoundaryValue::I32(2)));
    orchestrator.active_event_sequence = Some(EventSequence(1));
    orchestrator.complete_aggregate_member(coordinator_id, 0, Ok(BoundaryValue::I32(1)));

    assert_eq!(
        orchestrator.aggregate_coordinators[&coordinator_id]
            .winner
            .as_ref()
            .map(|(sequence, index, result)| (*sequence, *index, result.clone())),
        Some((EventSequence(1), 0, Ok(BoundaryValue::I32(1))))
    );
}

#[test]
pub(super) fn all_results_preserve_member_order_when_completions_arrive_out_of_order() {
    let mut orchestrator = Orchestrator::test_new();
    let coordinator_id = CoordinatorId::new(1);
    let thread_id = ThreadId::new(1);
    let module_id = ModuleId::new(1);
    orchestrator.aggregate_coordinators.insert(
        coordinator_id,
        AggregateCoordinator {
            mode: AggregateMode::All,
            future_ids: vec![FutureId::new(1), FutureId::new(2), FutureId::new(3)],
            pending: PendingContinuation {
                thread_id,
                continuation: galfus_vm::Continuation::for_provider(Reg(0), module_id, TypeIdx(0)),
                module_id,
                return_type: TypeIdx(0),
                stack: vec![],
                operation: PendingOperation::Future,
                active: Arc::new(AtomicBool::new(true)),
            },
            results: Some(vec![None, None, None]),
            remaining_results: 3,
            winner: None,
            armed: false,
        },
    );

    orchestrator.complete_aggregate_member(coordinator_id, 2, Ok(BoundaryValue::I32(3)));
    orchestrator.complete_aggregate_member(coordinator_id, 0, Ok(BoundaryValue::I32(1)));
    orchestrator.complete_aggregate_member(coordinator_id, 1, Ok(BoundaryValue::I32(2)));

    assert_eq!(
        orchestrator.aggregate_coordinators[&coordinator_id].results,
        Some(vec![
            Some(Ok(BoundaryValue::I32(1))),
            Some(Ok(BoundaryValue::I32(2))),
            Some(Ok(BoundaryValue::I32(3))),
        ])
    );
}

#[test]
pub(super) fn cancellation_event_removes_a_queued_thread() {
    let mut orchestrator = Orchestrator::test_new();
    let thread_id = {
        let kernel = orchestrator.kernel_mut();
        let thread_id = kernel
            .spawn(galfus_vm::thread::VmThreadState::test_new(), None)
            .unwrap();
        let thread = kernel
            .take_thread(thread_id)
            .expect("spawned thread is registered");
        kernel.enqueue_runnable(thread_id, thread).unwrap();
        thread_id
    };
    orchestrator.submit_event(RuntimeEvent::CancelThread { thread_id });

    orchestrator.process_events();

    assert_eq!(orchestrator.kernel().active_count(), 0);
    assert_eq!(orchestrator.kernel().runnable_count(), 0);
}

#[test]
pub(super) fn owner_exit_removes_all_of_its_future_records() {
    let mut orchestrator = Orchestrator::test_new();
    let thread_id = orchestrator
        .kernel_mut()
        .spawn(galfus_vm::thread::VmThreadState::test_new(), None)
        .unwrap();
    let future_id = FutureId::new(1);
    orchestrator
        .future_registry
        .create(
            thread_id,
            future_id,
            None,
            None,
            future_registry::Activation::Internal {
                operation: "test".to_string(),
                args: vec![],
            },
        )
        .unwrap();
    let thread = orchestrator
        .kernel_mut()
        .take_thread(thread_id)
        .expect("spawned thread is registered");
    orchestrator.submit_event(RuntimeEvent::Exited {
        thread_id,
        thread,
        result: Ok(BoundaryValue::Null),
    });

    orchestrator.process_events();

    assert!(
        orchestrator
            .future_registry
            .get(thread_id, future_id)
            .is_none()
    );
}

#[test]
pub(super) fn execution_cancellation_removes_every_thread_and_returns_a_structured_failure() {
    let mut orchestrator = Orchestrator::test_new();
    for _ in 0..2 {
        let kernel = orchestrator.kernel_mut();
        let thread_id = kernel
            .spawn(galfus_vm::thread::VmThreadState::test_new(), None)
            .unwrap();
        let thread = kernel
            .take_thread(thread_id)
            .expect("spawned thread is registered");
        kernel.enqueue_runnable(thread_id, thread).unwrap();
    }
    orchestrator.submit_event(RuntimeEvent::CancelExecution);

    match orchestrator.step(100) {
        galfus_contract::ThreadResult::Discarded => {}
        _ => panic!("execution cancellation must fail with Cancelled"),
    }
}

#[test]
pub(super) fn late_provider_completions_after_thread_cancellation_are_ignored() {
    let mut orchestrator = Orchestrator::test_new();
    let thread_id = orchestrator
        .kernel_mut()
        .spawn(galfus_vm::thread::VmThreadState::test_new(), None)
        .unwrap();
    orchestrator.submit_event(RuntimeEvent::CancelThread { thread_id });
    for _ in 0..2 {
        orchestrator.submit_event(RuntimeEvent::EffectCompleted {
            thread_id,
            request_lease: galfus_core::RequestLease::new(galfus_core::RequestId::new(1), 0),
            result: Err(galfus_contract::ExecutionFailure::new(
                galfus_contract::ExecutionFailureKind::ProviderFailure,
                "late provider completion",
            )),
        });
    }

    orchestrator.process_events();

    assert_eq!(orchestrator.kernel().active_count(), 0);
    assert!(orchestrator.failure.is_none());
    assert_eq!(orchestrator.late_completion_count(), 2);
}

#[test]
pub(super) fn execution_drops_orchestrator_and_sets_failed_state_on_error() {
    let mut orchestrator = Orchestrator::test_new();
    let thread_quota = Arc::new(galfus_vm::quota::ThreadQuota::new(
        orchestrator.quota.lock().unwrap().limits().clone(),
    ));
    let thread = galfus_vm::thread::VmThreadState::new(orchestrator.quota.clone(), thread_quota);
    let _owner = orchestrator.kernel.spawn(thread, None).unwrap();

    // Test logic here...
}

#[test]
pub(super) fn max_kernel_tasks_exhaustion_cancels_thread() {
    let limits = galfus_contract::LimitsMetadata {
        max_kernel_tasks: 0, // Trigger exhaustion immediately
        ..Default::default()
    };
    let quota = Arc::new(Mutex::new(galfus_vm::quota::GlobalQuota::new(
        limits.clone(),
    )));
    let mut orchestrator = Orchestrator::new(quota.clone());

    let thread_quota = Arc::new(galfus_vm::quota::ThreadQuota::new(limits.clone()));
    let thread = galfus_vm::thread::VmThreadState::new(quota.clone(), thread_quota);
    let thread_id = orchestrator.kernel.spawn(thread, None).unwrap();
    let taken_thread = orchestrator.kernel.take_thread(thread_id).unwrap();
    orchestrator
        .kernel
        .enqueue_runnable(thread_id, taken_thread)
        .unwrap();

    orchestrator.dispatch_runnables();

    assert!(orchestrator.failure.is_some());
    let failure = orchestrator.failure.unwrap();
    assert!(matches!(
        failure.kind,
        galfus_contract::ExecutionFailureKind::ResourceLimitExceeded {
            resource: galfus_contract::ResourceLimitKind::KernelTasks,
            ..
        }
    ));
}

#[test]
pub(super) fn max_runnable_threads_exhaustion_cancels_thread_on_unblock() {
    let limits = galfus_contract::LimitsMetadata {
        max_runnable_threads: 0, // Trigger exhaustion immediately
        ..Default::default()
    };
    let quota = Arc::new(Mutex::new(galfus_vm::quota::GlobalQuota::new(
        limits.clone(),
    )));
    let mut orchestrator = Orchestrator::new(quota.clone());

    let thread_quota = Arc::new(galfus_vm::quota::ThreadQuota::new(limits.clone()));
    let thread = galfus_vm::thread::VmThreadState::new(quota.clone(), thread_quota);
    let thread_id = orchestrator.kernel.spawn(thread, None).unwrap();

    // Block it first
    let taken_thread = orchestrator.kernel.take_thread(thread_id).unwrap();
    orchestrator
        .kernel
        .block(thread_id, taken_thread, None)
        .unwrap();

    // Now unblock it (which attempts to move to runnable)
    let result = orchestrator.kernel.unblock(thread_id);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(
        err,
        galfus_contract::ExecutionFailureKind::ResourceLimitExceeded {
            resource: galfus_contract::ResourceLimitKind::RunnableThreads,
            ..
        }
    ));
}

#[test]
pub(super) fn max_timers_exhaustion_fails_to_block() {
    let limits = galfus_contract::LimitsMetadata {
        max_timers: 0, // Trigger exhaustion immediately
        ..Default::default()
    };
    let quota = Arc::new(Mutex::new(galfus_vm::quota::GlobalQuota::new(
        limits.clone(),
    )));
    let mut orchestrator = Orchestrator::new(quota.clone());

    let thread_quota = Arc::new(galfus_vm::quota::ThreadQuota::new(limits.clone()));
    let thread = galfus_vm::thread::VmThreadState::new(quota.clone(), thread_quota);
    let thread_id = orchestrator.kernel.spawn(thread, None).unwrap();

    let taken_thread = orchestrator.kernel.take_thread(thread_id).unwrap();
    let result = orchestrator.kernel.block(thread_id, taken_thread, Some(10));
    assert!(result.is_err());
    let failure = result.unwrap_err();
    assert!(matches!(
        failure.kind,
        galfus_contract::ExecutionFailureKind::ResourceLimitExceeded {
            resource: galfus_contract::ResourceLimitKind::Timers,
            ..
        }
    ));
}

#[test]
pub(super) fn orchestrator_id_domains_fail_without_wrapping() {
    let mut orchestrator = Orchestrator::test_new();
    let thread_id = galfus_core::ThreadId::new(1);
    let thread = galfus_vm::thread::VmThreadState::test_new();

    assert!(
        orchestrator
            .allocate_request_lease(thread_id, galfus_core::FutureId::new(1), &thread)
            .is_some()
    );

    orchestrator
        .request_id_manager
        .set_next_id_for_test(u32::MAX);
    assert_eq!(
        orchestrator
            .allocate_request_lease(thread_id, galfus_core::FutureId::new(1), &thread)
            .unwrap()
            .id
            .raw(),
        u32::MAX
    );
    assert!(
        orchestrator
            .allocate_request_lease(thread_id, galfus_core::FutureId::new(1), &thread)
            .is_none()
    );
    assert_eq!(
        orchestrator.failure.as_ref().unwrap().kind,
        galfus_contract::ExecutionFailureKind::IdSpaceExhausted
    );

    orchestrator.failure = None;
    orchestrator
        .future_id_manager
        .set_next_id_for_test(u32::MAX);
    assert_eq!(
        orchestrator
            .allocate_future_lease(thread_id, &thread)
            .unwrap()
            .id
            .raw(),
        u32::MAX
    );
    assert!(
        orchestrator
            .allocate_future_lease(thread_id, &thread)
            .is_none()
    );

    orchestrator.failure = None;
    orchestrator
        .coordinator_id_manager
        .set_next_id_for_test(u32::MAX);
    assert_eq!(
        orchestrator
            .allocate_coordinator_id(thread_id, &thread)
            .unwrap()
            .raw(),
        u32::MAX
    );
    assert!(
        orchestrator
            .allocate_coordinator_id(thread_id, &thread)
            .is_none()
    );
    assert_eq!(
        orchestrator.failure.as_ref().unwrap().kind,
        galfus_contract::ExecutionFailureKind::IdSpaceExhausted
    );
}

#[test]
pub(super) fn generations_prevent_reuse_collisions() {
    let mut orchestrator = Orchestrator::test_new();
    let thread_id = galfus_core::ThreadId::new(1);
    let thread = galfus_vm::thread::VmThreadState::test_new();

    // Allocate first time
    let lease1 = orchestrator
        .allocate_request_lease(thread_id, galfus_core::FutureId::new(1), &thread)
        .unwrap();
    assert_eq!(lease1.generation, 1);

    // Free the ID manually for test
    orchestrator.request_id_manager.free(lease1.id);

    // Allocate again, should get same ID but higher generation
    let lease2 = orchestrator
        .allocate_request_lease(thread_id, galfus_core::FutureId::new(1), &thread)
        .unwrap();
    assert_eq!(lease1.id, lease2.id);
    assert_eq!(lease2.generation, 2);

    // Complete using the old lease, should be ignored
    orchestrator.submit_event(RuntimeEvent::EffectCompleted {
        thread_id,
        request_lease: lease1,
        result: Ok(galfus_contract::BoundaryValue::Null),
    });

    orchestrator.process_events();
    assert_eq!(orchestrator.late_completion_count(), 0); // ignored because generation mismatch
}
