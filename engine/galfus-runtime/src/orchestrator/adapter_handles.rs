use super::*;

use galfus_contract::{BoundaryValue, ExecutionFailureKind};

pub(super) fn collect_adapter_handles(
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

pub(super) fn stamp_adapter_handles(
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

impl Orchestrator {
    pub(super) fn register_adapter_handles(
        &mut self,
        proxy_module: &str,
        value: &BoundaryValue,
    ) -> Result<(), galfus_contract::ExecutionFailure> {
        let mut handles = Vec::new();
        collect_adapter_handles(value, &mut handles);
        if handles.is_empty() {
            return Ok(());
        }
        if let Err(e) = self
            .quota
            .lock()
            .unwrap()
            .try_reserve_external_handles(handles.len())
        {
            return Err(galfus_contract::ExecutionFailure::new(
                e,
                "external handles limit exceeded",
            ));
        }
        let Some(bindings) = &self.adapter_bindings else {
            self.quota
                .lock()
                .unwrap()
                .release_external_handles(handles.len());
            return Err(galfus_contract::ExecutionFailure::new(
                ExecutionFailureKind::BoundaryCodecFailure,
                "invalid handle: no adapter bindings",
            ));
        };
        let mut bindings = bindings.lock().unwrap();
        let Some(binding_id) = bindings.binding_id(proxy_module) else {
            self.quota
                .lock()
                .unwrap()
                .release_external_handles(handles.len());
            return Err(galfus_contract::ExecutionFailure::new(
                ExecutionFailureKind::BoundaryCodecFailure,
                "invalid handle: unknown proxy module",
            ));
        };
        if let Err(error) = bindings.register_handles(binding_id, &handles) {
            self.quota
                .lock()
                .unwrap()
                .release_external_handles(handles.len());
            let kind = match error {
                galfus_contract::AdapterBindingError::IdSpaceExhausted { .. } => {
                    ExecutionFailureKind::IdSpaceExhausted
                }
                galfus_contract::AdapterBindingError::DuplicateProxyModule(_)
                | galfus_contract::AdapterBindingError::InvalidHandle
                | galfus_contract::AdapterBindingError::HandlesStillActive => {
                    ExecutionFailureKind::BoundaryCodecFailure
                }
                galfus_contract::AdapterBindingError::CompensationReleaseFailed(_) => {
                    ExecutionFailureKind::AdapterCallFailure
                }
            };
            return Err(galfus_contract::ExecutionFailure::new(
                kind,
                error.to_string(),
            ));
        }
        Ok(())
    }
    pub(super) fn flush_thread_handle_drops(
        &mut self,
        thread: &mut galfus_vm::thread::VmThreadState,
    ) {
        let handles = std::mem::take(&mut thread.heap.pending_adapter_handle_drops);
        if let Err(error) = self.release_adapter_handles(handles) {
            self.failure = Some(ExecutionFailure::new(
                ExecutionFailureKind::AdapterCallFailure,
                error.to_string(),
            ));
        }
    }

    pub(super) fn teardown_thread_handles(
        &mut self,
        thread: &mut galfus_vm::thread::VmThreadState,
    ) {
        let handles = thread.extract_all_adapter_handles();
        if let Err(error) = self.release_adapter_handles(handles) {
            self.failure = Some(ExecutionFailure::new(
                ExecutionFailureKind::AdapterCallFailure,
                error.to_string(),
            ));
        }
    }

    pub(super) fn release_adapter_handles(
        &mut self,
        handles: Vec<(
            galfus_core::BindingId,
            galfus_core::OpaqueTypeId,
            galfus_core::HandleId,
        )>,
    ) -> Result<(), galfus_contract::AdapterBindingReleaseError> {
        for (binding_id, type_id, id) in handles {
            self.release_adapter_handle(binding_id, type_id, id)?;
        }
        Ok(())
    }

    pub(super) fn release_adapter_handle(
        &mut self,
        binding_id: galfus_core::BindingId,
        type_id: galfus_core::OpaqueTypeId,
        id: galfus_core::HandleId,
    ) -> Result<(), galfus_contract::AdapterBindingReleaseError> {
        let Some(bindings) = &self.adapter_bindings else {
            return Ok(());
        };
        let (release, module) = {
            let mut bindings = bindings
                .lock()
                .map_err(|_| galfus_contract::AdapterBindingReleaseError::RegistryPoisoned)?;
            let Some(release) = bindings.take_handle_for_release(binding_id, &type_id, id) else {
                return Ok(());
            };
            let module = bindings.take_module(release.proxy_module());
            (release, module)
        };
        self.quota.lock().unwrap().release_external_handles(1);
        let (outcome, module) = match module {
            Some(mut module) => {
                let outcome = module
                    .release_handle(release.type_id(), release.id())
                    .map_err(|error| {
                        galfus_contract::AdapterBindingReleaseError::AdapterReleaseFailed {
                            binding_id: release.binding_id(),
                            type_id: release.type_id().clone(),
                            id: release.id(),
                            error,
                        }
                    });
                (outcome, Some(module))
            }
            None => (
                Ok(galfus_contract::HandleReleaseOutcome::AlreadyReleased),
                None,
            ),
        };
        let mut bindings = bindings
            .lock()
            .map_err(|_| galfus_contract::AdapterBindingReleaseError::RegistryPoisoned)?;
        if let Some(module) = module {
            let _ = bindings.restore_module(release.proxy_module(), module);
        }
        match outcome {
            Ok(_) => Ok(()),
            Err(error) => {
                bindings.restore_handle_after_failed_release(release);
                Err(error)
            }
        }
    }

    pub(super) fn close_adapter_bindings(&mut self) -> AdapterBindingsCloseReport {
        let handles = match self.adapter_bindings.as_ref() {
            Some(bindings) => match bindings.lock() {
                Ok(bindings) => bindings.active_handles(),
                Err(_) => {
                    return AdapterBindingsCloseReport {
                        failures: vec![
                            galfus_contract::AdapterBindingReleaseError::RegistryPoisoned,
                        ],
                        ..AdapterBindingsCloseReport::default()
                    };
                }
            },
            None => return AdapterBindingsCloseReport::default(),
        };
        let mut report = AdapterBindingsCloseReport::default();
        for (binding_id, type_id, id) in handles {
            match self.release_adapter_handle(binding_id, type_id, id) {
                Ok(()) => report.released += 1,
                Err(error) => report.failures.push(error),
            }
        }
        report
    }
}
