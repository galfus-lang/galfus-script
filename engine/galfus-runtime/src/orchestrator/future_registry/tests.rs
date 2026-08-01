use super::*;
use std::sync::{Arc, atomic::AtomicBool};

fn owner() -> ThreadId {
    ThreadId::from_raw(1).unwrap()
}

fn foreign_owner() -> ThreadId {
    ThreadId::from_raw(2).unwrap()
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
            request_id: 7,
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
        .create(owner(), 7, Some(TypeIdx(3)), activation())
        .unwrap();
    registry
        .complete(owner(), 7, Ok(BoundaryValue::I32(42)))
        .unwrap();

    assert!(matches!(
        registry.add_waiter(owner(), 7, waiter()),
        Ok(WaitDisposition::Resolved {
            result: Ok(BoundaryValue::I32(42)),
            ..
        })
    ));
    assert!(matches!(
        registry.get(owner(), 7).map(|record| &record.state),
        Some(FutureState::Resolved(Ok(BoundaryValue::I32(42))))
    ));
}

#[test]
fn created_future_is_discarded_without_starting_its_activation() {
    let mut registry = FutureRegistry::new();
    registry.create(owner(), 7, None, activation()).unwrap();
    registry.discard(owner(), 7).unwrap();

    assert!(matches!(
        registry.take_activation_for_start(owner(), 7),
        Ok(None)
    ));
    assert!(matches!(
        registry.add_waiter(owner(), 7, waiter()),
        Ok(WaitDisposition::Discarded)
    ));
}

#[test]
fn duplicate_completion_is_rejected_without_replacing_the_cache() {
    let mut registry = FutureRegistry::new();
    registry.create(owner(), 7, None, activation()).unwrap();
    registry
        .complete(owner(), 7, Ok(BoundaryValue::I32(1)))
        .unwrap();

    assert!(
        registry
            .complete(owner(), 7, Ok(BoundaryValue::I32(2)))
            .is_err()
    );
    assert!(matches!(
        registry.get(owner(), 7).map(|record| &record.state),
        Some(FutureState::Resolved(Ok(BoundaryValue::I32(1))))
    ));
}

#[test]
fn discarded_future_cannot_be_completed_later() {
    let mut registry = FutureRegistry::new();
    registry.create(owner(), 7, None, activation()).unwrap();
    registry.discard(owner(), 7).unwrap();

    assert!(
        registry
            .complete(owner(), 7, Ok(BoundaryValue::I32(42)))
            .is_err()
    );
    assert!(matches!(
        registry.get(owner(), 7).map(|record| &record.state),
        Some(FutureState::Discarded)
    ));
}

#[test]
fn completion_drains_all_registered_waiters_once() {
    let mut registry = FutureRegistry::new();
    registry.create(owner(), 7, None, activation()).unwrap();
    assert!(matches!(
        registry.add_waiter(owner(), 7, waiter()),
        Ok(WaitDisposition::Registered)
    ));
    assert!(matches!(
        registry.add_waiter(owner(), 7, waiter()),
        Ok(WaitDisposition::Registered)
    ));

    let waiters = registry
        .complete(owner(), 7, Ok(BoundaryValue::I32(42)))
        .unwrap();
    assert_eq!(waiters.len(), 2);
    assert!(matches!(
        registry.add_waiter(owner(), 7, waiter()),
        Ok(WaitDisposition::Resolved {
            result: Ok(BoundaryValue::I32(42)),
            ..
        })
    ));
}

#[test]
fn duplicate_ids_and_foreign_owners_are_rejected() {
    let mut registry = FutureRegistry::new();
    registry.create(owner(), 7, None, activation()).unwrap();

    assert!(registry.create(owner(), 7, None, activation()).is_err());
    assert!(registry.add_waiter(foreign_owner(), 7, waiter()).is_err());
    assert!(
        registry
            .take_activation_for_start(foreign_owner(), 7)
            .is_err()
    );
}
