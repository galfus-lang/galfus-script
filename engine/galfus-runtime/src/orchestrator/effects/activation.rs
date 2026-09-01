use super::*;

use crate::orchestrator::future_registry::Activation;
use crate::task::execution_stack;
use galfus_bytecode::instruction::FuncIdx;
use galfus_contract::{ExecutionFailure, ExecutionFailureKind};
use galfus_core::ModuleId;

impl Orchestrator {
    pub(super) fn start_activation(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        future_id: galfus_core::FutureId,
        activation: Activation,
        aggregate_registration: Option<(galfus_core::CoordinatorId, usize)>,
    ) -> Option<galfus_vm::thread::VmThreadState> {
        match activation {
            Activation::GalfusFunction {
                module_id,
                func_idx,
                args,
                arg_types,
            } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.galfus_activations += 1;
                }
                self.start_galfus_function_activation(
                    thread_id, thread, future_id, module_id, func_idx, args, arg_types,
                )
            }
            Activation::Provider {
                alias, name, args, ..
            } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.provider_activations += 1;
                }
                self.start_provider_activation(thread_id, thread, future_id, alias, name, args)
            }
            Activation::Adapter {
                proxy_module,
                symbol,
                args,
                ..
            } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.adapter_activations += 1;
                }
                self.start_adapter_activation(
                    thread_id,
                    thread,
                    future_id,
                    proxy_module,
                    symbol,
                    args,
                )
            }
            Activation::Internal { operation, args } => {
                #[cfg(feature = "metrics")]
                {
                    self.future_metrics.internal_activations += 1;
                }
                self.start_internal_activation(
                    thread_id,
                    thread,
                    future_id,
                    operation,
                    args,
                    aggregate_registration,
                )
            }
        }
    }

    pub(super) fn start_galfus_function_activation(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        future_id: galfus_core::FutureId,
        act_module_id: ModuleId,
        func_idx: FuncIdx,
        args: Vec<galfus_contract::BoundaryValue>,
        arg_types: Box<[galfus_bytecode::instruction::TypeIdx]>,
    ) -> Option<galfus_vm::thread::VmThreadState> {
        let thread_quota = std::sync::Arc::new(galfus_vm::quota::ThreadQuota::new(
            self.quota.lock().unwrap().limits().clone(),
        ));
        let mut worker_thread =
            galfus_vm::thread::VmThreadState::new(self.quota.clone(), thread_quota);
        let module = &self
            .vm
            .as_ref()
            .unwrap()
            .graph
            .get(act_module_id)
            .unwrap()
            .module;
        let receiver_layout = module.functions[func_idx.raw() as usize]
            .name
            .split_once("::")
            .and_then(|(owner, _)| {
                module
                    .struct_layouts
                    .iter()
                    .position(|layout| layout.name == owner)
                    .map(|index| {
                        (
                            galfus_bytecode::StructLayoutIdx(index as u16),
                            &module.struct_layouts[index],
                        )
                    })
            });

        let mut vm_args = Vec::with_capacity(args.len());
        for (index, (boundary, expected_ty)) in args.into_iter().zip(arg_types).enumerate() {
            let vm_val = match match (index, &boundary, receiver_layout) {
                (0, galfus_contract::BoundaryValue::Tuple(values), Some((layout_idx, layout))) => {
                    (|| {
                        if values.len() != layout.fields.len() {
                            return Err(galfus_contract::BoundaryCodecError::UnsupportedType);
                        }
                        let fields = values
                            .iter()
                            .cloned()
                            .zip(layout.fields.iter())
                            .map(|(value, field)| {
                                crate::task::encode_into_thread_heap(
                                    &mut worker_thread.heap,
                                    value,
                                    field.ty,
                                    act_module_id,
                                    module,
                                )
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        let reference = worker_thread
                            .heap
                            .alloc(galfus_vm::HeapObject::Struct {
                                module_id: act_module_id,
                                layout_idx,
                                fields,
                            })
                            .map_err(|_| galfus_contract::BoundaryCodecError::HeapExhausted)?;
                        Ok(galfus_vm::VmValue::Object(reference))
                    })()
                }
                _ => crate::task::encode_into_thread_heap(
                    &mut worker_thread.heap,
                    boundary,
                    expected_ty,
                    act_module_id,
                    module,
                ),
            } {
                Ok(value) => value,
                Err(error) => {
                    self.failure = Some(
                        ExecutionFailure::new(
                            ExecutionFailureKind::BoundaryCodecFailure,
                            format!("invalid future worker argument: {error:?}"),
                        )
                        .with_thread_id(thread_id)
                        .with_module_id(act_module_id.raw().into())
                        .with_stack(execution_stack(&thread)),
                    );
                    self.kernel.cancel(thread_id);
                    return None;
                }
            };
            vm_args.push(vm_val);
        }
        if let Err(error) = self
            .vm
            .as_ref()
            .expect("VM is configured before execution")
            .prepare_function(&mut worker_thread, act_module_id, func_idx, vm_args)
        {
            self.failure = Some(
                ExecutionFailure::new(ExecutionFailureKind::VmPanic, error.to_string())
                    .with_thread_id(thread_id)
                    .with_module_id(act_module_id.raw().into())
                    .with_stack(execution_stack(&thread)),
            );
            self.kernel.cancel(thread_id);
            return None;
        }

        let worker_id = match self.kernel.spawn(worker_thread, None) {
            Ok(worker_id) => worker_id,
            Err(error) => {
                self.failure = Some(
                    error
                        .with_thread_id(thread_id)
                        .with_future_id(future_id)
                        .with_stack(execution_stack(&thread)),
                );
                self.kernel.cancel(thread_id);
                return None;
            }
        };
        self.future_workers.insert(
            worker_id,
            (
                thread_id,
                galfus_core::FutureLease::new(
                    future_id,
                    self.future_generations
                        .get(&future_id.raw())
                        .copied()
                        .unwrap_or(0),
                ),
            ),
        );
        let spawned_thread = self.kernel.take_thread(worker_id).unwrap();
        if let Err(e) = self.kernel.enqueue_runnable(worker_id, spawned_thread) {
            self.failure = Some(
                ExecutionFailure::new(e, "runnable threads limit exceeded")
                    .with_thread_id(worker_id),
            );
            self.kernel.cancel(worker_id);
        }
        Some(thread)
    }
}
