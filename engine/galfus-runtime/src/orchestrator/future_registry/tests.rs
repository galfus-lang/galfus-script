use super::*;
use std::sync::{Arc, atomic::AtomicBool};

fn owner() -> ThreadId {
    galfus_core::ThreadId::new(1)
}

fn foreign_owner() -> ThreadId {
    galfus_core::ThreadId::new(2)
}

fn activation() -> Activation {
    Activation::GalfusFunction {
        module_id: ModuleId::new(1),
        func_idx: FuncIdx(0),
        args: vec![],
        arg_types: vec![],
    }
}

fn waiter() -> Waiter {
    Waiter {
        continuation: PendingContinuation {
            thread_id: owner(),
            continuation: galfus_vm::Continuation::for_provider(
                galfus_bytecode::instruction::Reg(0),
                ModuleId::new(1),
                TypeIdx(0),
            ),
            module_id: ModuleId::new(1),
            return_type: TypeIdx(0),
            stack: vec![],
            operation: super::super::pending::PendingOperation::Future,
            active: Arc::new(AtomicBool::new(true)),
        },
    }
}

#[test]
fn resolved_future_keeps_its_cached_result_for_later_awaits() {
    let mut registry = FutureRegistry::new();
    registry
        .create(
            owner(),
            galfus_core::FutureId::new(7),
            Some(TypeIdx(3)),
            Some(ModuleId::new(1)),
            activation(),
        )
        .unwrap();
    registry
        .complete(
            owner(),
            galfus_core::FutureId::new(7),
            Ok(BoundaryValue::I32(42)),
        )
        .unwrap();

    assert!(matches!(
        registry.add_waiter(owner(), galfus_core::FutureId::new(7), waiter()),
        Ok(WaitDisposition::Resolved {
            result: Ok(BoundaryValue::I32(42)),
            ..
        })
    ));
    assert!(matches!(
        registry
            .get(owner(), galfus_core::FutureId::new(7))
            .map(|record| &record.state),
        Some(FutureState::Resolved(Ok(BoundaryValue::I32(42))))
    ));
}

#[test]
fn created_future_is_discarded_without_starting_its_activation() {
    let mut registry = FutureRegistry::new();
    registry
        .create(
            owner(),
            galfus_core::FutureId::new(7),
            None,
            None,
            activation(),
        )
        .unwrap();
    registry
        .discard(owner(), galfus_core::FutureId::new(7))
        .unwrap();

    assert!(matches!(
        registry.take_activation_for_start(owner(), galfus_core::FutureId::new(7)),
        Err(_)
    ));
    assert!(matches!(
        registry.add_waiter(owner(), galfus_core::FutureId::new(7), waiter()),
        Err(_)
    ));
}

#[test]
fn running_future_is_discarded_and_exposes_its_activation_for_cancellation() {
    let mut registry = FutureRegistry::new();
    registry
        .create(
            owner(),
            galfus_core::FutureId::new(7),
            None,
            None,
            activation(),
        )
        .unwrap();
    registry
        .take_activation_for_start(owner(), galfus_core::FutureId::new(7))
        .unwrap();

    let disp = registry.discard(owner(), galfus_core::FutureId::new(7));
    // Record is removed, but we can verify it was returned
    assert!(matches!(
        disp,
        Ok(DiscardDisposition::Running(
            Activation::GalfusFunction { .. }
        ))
    ));
}

#[test]
fn duplicate_completion_is_rejected_without_replacing_the_cache() {
    let mut registry = FutureRegistry::new();
    registry
        .create(
            owner(),
            galfus_core::FutureId::new(7),
            None,
            None,
            activation(),
        )
        .unwrap();
    registry
        .complete(
            owner(),
            galfus_core::FutureId::new(7),
            Ok(BoundaryValue::I32(1)),
        )
        .unwrap();

    assert!(
        registry
            .complete(
                owner(),
                galfus_core::FutureId::new(7),
                Ok(BoundaryValue::I32(2))
            )
            .is_err()
    );
    assert!(matches!(
        registry
            .get(owner(), galfus_core::FutureId::new(7))
            .map(|record| &record.state),
        Some(FutureState::Resolved(Ok(BoundaryValue::I32(1))))
    ));
}

#[test]
fn discarded_future_cannot_be_completed_later() {
    let mut registry = FutureRegistry::new();
    registry
        .create(
            owner(),
            galfus_core::FutureId::new(7),
            None,
            None,
            activation(),
        )
        .unwrap();
    registry
        .discard(owner(), galfus_core::FutureId::new(7))
        .unwrap();

    assert!(
        registry
            .complete(
                owner(),
                galfus_core::FutureId::new(7),
                Ok(BoundaryValue::I32(42))
            )
            .is_err()
    );
    assert!(
        registry
            .get(owner(), galfus_core::FutureId::new(7))
            .is_none()
    );
}

#[test]
fn completion_drains_all_registered_waiters_once() {
    let mut registry = FutureRegistry::new();
    registry
        .create(
            owner(),
            galfus_core::FutureId::new(7),
            None,
            None,
            activation(),
        )
        .unwrap();
    assert!(matches!(
        registry.add_waiter(owner(), galfus_core::FutureId::new(7), waiter()),
        Ok(WaitDisposition::Registered)
    ));
    assert!(matches!(
        registry.add_waiter(owner(), galfus_core::FutureId::new(7), waiter()),
        Ok(WaitDisposition::Registered)
    ));

    let waiters = registry
        .complete(
            owner(),
            galfus_core::FutureId::new(7),
            Ok(BoundaryValue::I32(42)),
        )
        .unwrap();
    assert_eq!(waiters.len(), 2);
    assert!(matches!(
        registry.add_waiter(owner(), galfus_core::FutureId::new(7), waiter()),
        Ok(WaitDisposition::Resolved {
            result: Ok(BoundaryValue::I32(42)),
            ..
        })
    ));
}

#[test]
fn duplicate_ids_and_foreign_owners_are_rejected() {
    let mut registry = FutureRegistry::new();
    registry
        .create(
            owner(),
            galfus_core::FutureId::new(7),
            None,
            None,
            activation(),
        )
        .unwrap();

    assert!(
        registry
            .create(
                owner(),
                galfus_core::FutureId::new(7),
                None,
                None,
                activation()
            )
            .is_err()
    );
    assert!(
        registry
            .add_waiter(foreign_owner(), galfus_core::FutureId::new(7), waiter())
            .is_err()
    );
    assert!(
        registry
            .take_activation_for_start(foreign_owner(), galfus_core::FutureId::new(7))
            .is_err()
    );
}

#[test]
fn dropping_its_final_handle_removes_its_registry_record() {
    let mut registry = FutureRegistry::new();
    registry
        .create(
            owner(),
            galfus_core::FutureId::new(7),
            None,
            None,
            activation(),
        )
        .unwrap();
    registry
        .complete(
            owner(),
            galfus_core::FutureId::new(7),
            Ok(BoundaryValue::I32(42)),
        )
        .unwrap();

    let _ = registry
        .discard(owner(), galfus_core::FutureId::new(7))
        .unwrap();

    // In Phase 1, discarding a resolved future should remove it from the registry.
    assert!(
        registry
            .get(owner(), galfus_core::FutureId::new(7))
            .is_none()
    );
}

#[test]
fn owner_shutdown_releases_all_future_payloads_and_keeps_tombstones() {
    let mut registry = FutureRegistry::new();
    for future_id in [galfus_core::FutureId::new(7), galfus_core::FutureId::new(8)] {
        registry
            .create(owner(), future_id, None, None, activation())
            .unwrap();
    }
    registry
        .complete(
            owner(),
            galfus_core::FutureId::new(7),
            Ok(BoundaryValue::I32(42)),
        )
        .unwrap();
    registry
        .take_activation_for_start(owner(), galfus_core::FutureId::new(8))
        .unwrap();

    let discarded = registry.discard_all_for_owner(owner());

    assert_eq!(discarded.len(), 2);
    assert!(
        registry
            .get(owner(), galfus_core::FutureId::new(7))
            .is_none()
    );
    assert!(
        registry
            .get(owner(), galfus_core::FutureId::new(8))
            .is_none()
    );
    for future_id in [galfus_core::FutureId::new(7), galfus_core::FutureId::new(8)] {
        let error = match registry.complete(owner(), future_id, Ok(BoundaryValue::Null)) {
            Err(error) => error,
            Ok(_) => panic!("shutdown future must reject a late completion"),
        };
        assert_eq!(
            error.kind,
            galfus_contract::ExecutionFailureKind::DuplicateCompletion
        );
    }
}

#[test]
fn late_completion_after_discard_is_ignored_and_recorded_deterministically() {
    let mut registry = FutureRegistry::new();
    registry
        .create(
            owner(),
            galfus_core::FutureId::new(7),
            None,
            None,
            activation(),
        )
        .unwrap();
    registry
        .discard(owner(), galfus_core::FutureId::new(7))
        .unwrap();

    let err = match registry.complete(
        owner(),
        galfus_core::FutureId::new(7),
        Ok(BoundaryValue::I32(42)),
    ) {
        Err(e) => e,
        Ok(_) => panic!("Expected error"),
    };

    assert_eq!(
        err.kind,
        galfus_contract::ExecutionFailureKind::DuplicateCompletion
    );

    // The state should remain Discarded or a tombstone variant (to be implemented in Phase 1)
    assert!(matches!(
        registry
            .get(owner(), galfus_core::FutureId::new(7))
            .map(|record| &record.state),
        Some(FutureState::Discarded) | None
    ));
}

#[test]
fn completion_and_terminal_actions_have_one_deterministic_outcome_in_every_order() {
    #[derive(Clone, Copy)]
    enum Action {
        Complete,
        Cancel,
        OwnerExit,
        Shutdown,
    }

    fn apply(registry: &mut FutureRegistry, action: Action) -> (bool, usize) {
        let future_id = galfus_core::FutureId::new(7);
        match action {
            Action::Complete => registry
                .complete(owner(), future_id, Ok(BoundaryValue::Null))
                .map(|_| (true, 0))
                .unwrap_or((false, 0)),
            Action::Cancel => registry
                .discard(owner(), future_id)
                .map(|disposition| {
                    (
                        true,
                        usize::from(matches!(disposition, DiscardDisposition::Running(_))),
                    )
                })
                .unwrap_or((false, 0)),
            Action::OwnerExit => {
                let discarded = registry.discard_all_for_owner(owner());
                (
                    !discarded.is_empty(),
                    discarded
                        .iter()
                        .filter(|(_, activation)| activation.is_some())
                        .count(),
                )
            }
            Action::Shutdown => {
                let discarded = registry.discard_all();
                (
                    !discarded.is_empty(),
                    discarded
                        .iter()
                        .filter(|(_, _, activation)| activation.is_some())
                        .count(),
                )
            }
        }
    }

    let actions = [
        Action::Complete,
        Action::Cancel,
        Action::OwnerExit,
        Action::Shutdown,
    ];
    for first in actions {
        for second in actions {
            let mut registry = FutureRegistry::new();
            let future_id = galfus_core::FutureId::new(7);
            registry
                .create(owner(), future_id, None, None, activation())
                .expect("future registers");
            registry
                .take_activation_for_start(owner(), future_id)
                .expect("future starts");

            let (first_applied, first_cancellations) = apply(&mut registry, first);
            let (_, second_cancellations) = apply(&mut registry, second);
            assert!(first_applied);
            assert!(
                first_cancellations + second_cancellations <= 1,
                "a running request must only be cancelled once"
            );
            assert!(
                registry
                    .complete(owner(), future_id, Ok(BoundaryValue::Null))
                    .is_err(),
                "every action order must leave the request in one terminal state"
            );
        }
    }
}
