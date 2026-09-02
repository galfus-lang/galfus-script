use super::*;

use crate::task::{decode_surface_from_thread_heap, execution_stack};
use galfus_bytecode::instruction::{FuncIdx, TypeIdx};
use galfus_contract::{BoundaryValue, ExecutionFailure, ExecutionFailureKind};
use galfus_core::ModuleId;

impl Orchestrator {
    pub(super) fn handle_create_await_future(
        &mut self,
        thread_id: crate::registry::ThreadId,
        mut thread: galfus_vm::thread::VmThreadState,
        continuation: galfus_vm::Continuation,
        module_id: ModuleId,
        operation: Box<str>,
        args: Vec<galfus_vm::VmValue>,
        arg_types: &[TypeIdx],
        return_type: TypeIdx,
    ) {
        let encoded_args = {
            let module = &self
                .vm
                .as_ref()
                .unwrap()
                .graph
                .get(module_id)
                .unwrap()
                .module;
            let mut encoded_args = Vec::with_capacity(args.len());
            for (arg, ty) in args.into_iter().zip(arg_types.iter()) {
                match crate::task::decode_from_thread_heap(&thread.heap, arg, *ty, module) {
                    Ok(value) => encoded_args.push(value),
                    Err(_) if matches!(arg, galfus_vm::VmValue::Function { .. }) => {
                        let galfus_vm::VmValue::Function {
                            module_id,
                            func_idx,
                        } = arg
                        else {
                            unreachable!();
                        };
                        encoded_args.push(BoundaryValue::Function {
                            module_id: module_id.raw(),
                            func_idx: func_idx.raw(),
                        });
                    }
                    Err(error) => {
                        self.failure = Some(
                            ExecutionFailure::new(
                                ExecutionFailureKind::BoundaryCodecFailure,
                                format!("invalid future argument: {error:?}"),
                            )
                            .with_thread_id(thread_id)
                            .with_module_id(module_id.raw().into())
                            .with_stack(execution_stack(&thread)),
                        );
                        self.kernel.cancel(thread_id);
                        return;
                    }
                };
            }
            encoded_args
        };

        if let Some(result) = self.try_complete_internal_await(thread_id, &operation, &encoded_args)
        {
            let module = &self
                .vm
                .as_ref()
                .unwrap()
                .graph
                .get(module_id)
                .unwrap()
                .module;
            let value = match result.and_then(|value| {
                crate::task::encode_into_thread_heap(
                    &mut thread.heap,
                    value,
                    return_type,
                    module_id,
                    module,
                )
                .map_err(|error| {
                    ExecutionFailure::new(
                        ExecutionFailureKind::BoundaryCodecFailure,
                        format!("invalid asynchronous result: {error:?}"),
                    )
                })
            }) {
                Ok(value) => value,
                Err(error) => {
                    self.failure = Some(
                        error
                            .with_thread_id(thread_id)
                            .with_stack(execution_stack(&thread)),
                    );
                    self.kernel.cancel(thread_id);
                    return;
                }
            };
            #[cfg(feature = "metrics")]
            {
                self.future_metrics.internal_await_immediate += 1;
            }
            self.resume_or_fail_front(thread_id, thread, continuation, value);
            return;
        }

        #[cfg(feature = "metrics")]
        {
            self.future_metrics.created += 1;
            self.future_metrics.boundary_arguments += encoded_args.len();
            self.future_metrics.internal_await_suspended += 1;
        }
        let Some(future_lease) = self.allocate_future_lease(thread_id, &thread) else {
            return;
        };
        let future_id = future_lease.id;

        let activation = crate::orchestrator::future_registry::Activation::Internal {
            operation: operation.into(),
            args: encoded_args,
        };
        if let Err(error) = self.future_registry.insert_direct_await(
            thread_id,
            future_id,
            return_type,
            module_id,
            activation,
        ) {
            self.failure = Some(error.with_stack(execution_stack(&thread)));
            self.kernel.cancel(thread_id);
            return;
        }
        self.handle_future_wait(
            thread_id,
            thread,
            continuation,
            future_id,
            module_id,
            return_type,
        );
    }

    #[allow(clippy::boxed_local)]
    pub(super) fn handle_create_future(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        continuation: galfus_vm::Continuation,
        module_id: ModuleId,
        target_module_id: ModuleId,
        func_idx: FuncIdx,
        args: Vec<galfus_vm::VmValue>,
        arg_types: Box<[TypeIdx]>,
        return_type: TypeIdx,
    ) {
        #[cfg(feature = "metrics")]
        {
            self.future_metrics.created += 1;
            self.future_metrics.boundary_arguments += args.len();
        }
        let Some(future_lease) = self.allocate_future_lease(thread_id, &thread) else {
            return;
        };
        let future_id = future_lease.id;

        let module = &self
            .vm
            .as_ref()
            .unwrap()
            .graph
            .get(module_id)
            .unwrap()
            .module;
        let activation_result = self.future_activation(
            target_module_id,
            func_idx,
            args.clone(),
            || {
                let mut encoded_args = Vec::with_capacity(args.len());
                for (arg, ty) in args.clone().into_iter().zip(arg_types.iter()) {
                    match crate::task::decode_from_thread_heap(&thread.heap, arg, *ty, module) {
                        Ok(value) => encoded_args.push(value),
                        Err(_) if matches!(arg, galfus_vm::VmValue::Function { .. }) => {
                            let galfus_vm::VmValue::Function {
                                module_id,
                                func_idx,
                            } = arg
                            else {
                                unreachable!();
                            };
                            encoded_args.push(BoundaryValue::Function {
                                module_id: module_id.raw(),
                                func_idx: func_idx.raw(),
                            });
                            continue;
                        }
                        Err(error) => return Err(error),
                    };
                }
                Ok(encoded_args)
            },
            |contracts| {
                args.iter()
                    .cloned()
                    .zip(arg_types.iter().zip(contracts.iter()))
                    .map(|(arg, (ty, contract))| {
                        decode_surface_from_thread_heap(
                            &thread.heap,
                            &contract.schema,
                            arg,
                            *ty,
                            module,
                        )
                    })
                    .collect()
            },
        );

        let activation = match activation_result {
            Ok(activation) => activation,
            Err(error) => {
                self.failure = Some(
                    ExecutionFailure::new(
                        ExecutionFailureKind::BoundaryCodecFailure,
                        format!("invalid future argument: {error:?}"),
                    )
                    .with_thread_id(thread_id)
                    .with_module_id(module_id.raw().into())
                    .with_stack(execution_stack(&thread)),
                );
                self.kernel.cancel(thread_id);
                return;
            }
        };
        if let Err(error) = self.future_registry.insert_created(
            thread_id,
            future_id,
            Some(return_type),
            Some(module_id),
            activation,
        ) {
            self.failure = Some(error.with_stack(execution_stack(&thread)));
            self.kernel.cancel(thread_id);
            return;
        }

        self.resume_or_fail_front(
            thread_id,
            thread,
            continuation,
            galfus_vm::VmValue::Future(future_id),
        );
    }

    #[allow(clippy::boxed_local)]
    pub(super) fn handle_create_indirect_future(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: galfus_vm::thread::VmThreadState,
        continuation: galfus_vm::Continuation,
        module_id: ModuleId,
        func: galfus_vm::VmValue,
        args: Vec<galfus_vm::VmValue>,
        arg_types: Box<[TypeIdx]>,
        return_type: TypeIdx,
    ) {
        #[cfg(feature = "metrics")]
        {
            self.future_metrics.created += 1;
            self.future_metrics.boundary_arguments += args.len();
        }
        let Some(future_lease) = self.allocate_future_lease(thread_id, &thread) else {
            return;
        };
        let future_id = future_lease.id;
        let galfus_vm::VmValue::Function {
            module_id: target_module_id,
            func_idx,
        } = func
        else {
            self.failure = Some(
                ExecutionFailure::new(
                    ExecutionFailureKind::InvalidContinuation,
                    "indirect async call requires a function value",
                )
                .with_thread_id(thread_id)
                .with_module_id(module_id.raw().into())
                .with_stack(execution_stack(&thread)),
            );
            self.kernel.cancel(thread_id);
            return;
        };
        let target_module = &self
            .vm
            .as_ref()
            .unwrap()
            .graph
            .get(target_module_id)
            .unwrap()
            .module;
        let activation_result = self.future_activation(
            target_module_id,
            func_idx,
            args.clone(),
            || {
                let mut encoded_args = Vec::with_capacity(args.len());
                for (arg, ty) in args.clone().into_iter().zip(arg_types.iter()) {
                    match crate::task::decode_from_thread_heap(
                        &thread.heap,
                        arg,
                        *ty,
                        target_module,
                    ) {
                        Ok(value) => encoded_args.push(value),
                        Err(error) => return Err(error),
                    }
                }
                Ok(encoded_args)
            },
            |contracts| {
                args.iter()
                    .cloned()
                    .zip(arg_types.iter().zip(contracts.iter()))
                    .map(|(arg, (ty, contract))| {
                        decode_surface_from_thread_heap(
                            &thread.heap,
                            &contract.schema,
                            arg,
                            *ty,
                            target_module,
                        )
                    })
                    .collect()
            },
        );

        let activation = match activation_result {
            Ok(activation) => activation,
            Err(error) => {
                self.failure = Some(
                    ExecutionFailure::new(
                        ExecutionFailureKind::BoundaryCodecFailure,
                        format!("invalid indirect future argument: {error:?}"),
                    )
                    .with_thread_id(thread_id)
                    .with_module_id(module_id.raw().into())
                    .with_stack(execution_stack(&thread)),
                );
                self.kernel.cancel(thread_id);
                return;
            }
        };
        if let Err(error) = self.future_registry.insert_created(
            thread_id,
            future_id,
            Some(return_type),
            Some(module_id),
            activation,
        ) {
            self.failure = Some(error.with_stack(execution_stack(&thread)));
            self.kernel.cancel(thread_id);
            return;
        }
        self.resume_or_fail_front(
            thread_id,
            thread,
            continuation,
            galfus_vm::VmValue::Future(future_id),
        );
    }

    pub(crate) fn allocate_request_lease(
        &mut self,
        thread_id: crate::registry::ThreadId,
        future_id: galfus_core::FutureId,
        thread: &galfus_vm::thread::VmThreadState,
    ) -> Option<galfus_core::RequestLease> {
        if let Err(e) = self.quota.lock().unwrap().try_reserve_pending_requests(1) {
            self.failure = Some(
                galfus_contract::ExecutionFailure::new(e, "request quota exceeded")
                    .with_thread_id(thread_id)
                    .with_future_id(future_id)
                    .with_stack(crate::orchestrator::execution_stack(thread)),
            );
            self.kernel.cancel(thread_id);
            return None;
        }

        if let Some(id) = self.request_id_manager.try_allocate() {
            let gen_val = self
                .request_generations
                .entry(id.raw())
                .and_modify(|g| *g = g.wrapping_add(1))
                .or_insert(1);
            Some(galfus_core::RequestLease::new(id, *gen_val))
        } else {
            self.quota.lock().unwrap().release_pending_requests(1);
            self.failure = Some(
                ExecutionFailure::new(
                    ExecutionFailureKind::IdSpaceExhausted,
                    "request id space exhausted",
                )
                .with_thread_id(thread_id)
                .with_future_id(future_id)
                .with_stack(execution_stack(thread)),
            );
            self.kernel.cancel(thread_id);
            None
        }
    }

    pub(crate) fn allocate_future_lease(
        &mut self,
        thread_id: crate::registry::ThreadId,
        thread: &galfus_vm::thread::VmThreadState,
    ) -> Option<galfus_core::FutureLease> {
        if let Err(e) = self.quota.lock().unwrap().try_reserve_futures(1) {
            self.failure = Some(
                galfus_contract::ExecutionFailure::new(e, "future quota exceeded")
                    .with_thread_id(thread_id)
                    .with_stack(crate::orchestrator::execution_stack(thread)),
            );
            self.kernel.cancel(thread_id);
            return None;
        }

        if let Some(id) = self.future_id_manager.try_allocate() {
            let gen_val = self
                .future_generations
                .entry(id.raw())
                .and_modify(|g| *g = g.wrapping_add(1))
                .or_insert(1);
            Some(galfus_core::FutureLease::new(id, *gen_val))
        } else {
            self.quota.lock().unwrap().release_futures(1);
            self.failure = Some(
                ExecutionFailure::new(
                    ExecutionFailureKind::IdSpaceExhausted,
                    "future id space exhausted",
                )
                .with_thread_id(thread_id)
                .with_stack(execution_stack(thread)),
            );
            self.kernel.cancel(thread_id);
            None
        }
    }

    pub(super) fn future_activation(
        &self,
        target_module_id: ModuleId,
        func_idx: FuncIdx,
        args_vm: Vec<galfus_vm::VmValue>,
        encoded_args: impl FnOnce() -> Result<Vec<BoundaryValue>, galfus_contract::BoundaryCodecError>,
        encoded_surface_args: impl FnOnce(
            &[galfus_contract::SurfaceContract],
        ) -> Result<Vec<galfus_contract::SurfaceValue>, String>,
    ) -> Result<crate::orchestrator::future_registry::Activation, String> {
        let target = &self
            .vm
            .as_ref()
            .expect("VM is configured before execution")
            .graph
            .get(target_module_id)
            .expect("future target module is loaded")
            .module;
        let function_name = target.functions[func_idx.raw() as usize].name.clone();
        let adapter_identity = target.functions[func_idx.raw() as usize]
            .adapter_proxy_metadata
            .as_ref()
            .map(|meta| (meta.proxy_module.clone(), meta.symbol.clone()));

        if let Some(name) = function_name.strip_prefix("__provider_") {
            let alias = galfus_contract::provider_alias_from_operation(name)
                .expect("compiled provider operations have a valid alias")
                .to_string();
            let surface_contract = self.provider_surface_contract(&alias, name);
            let contract = surface_contract
                .ok_or_else(|| format!("provider operation {name} has no surface contract"))?;
            if contract.parameters.len() != args_vm.len() {
                return Err(format!(
                    "surface contract {} expects {} arguments, received {}",
                    contract.bridge_symbol,
                    contract.parameters.len(),
                    args_vm.len(),
                ));
            }
            let args = crate::orchestrator::future_registry::ProviderArguments::Surface(
                encoded_surface_args(&contract.parameters)?,
            );
            Ok(crate::orchestrator::future_registry::Activation::Provider {
                alias,
                name: name.to_string(),
                args,
                request_id: None,
            })
        } else if function_name.starts_with("__internal_") {
            Ok(crate::orchestrator::future_registry::Activation::Internal {
                operation: function_name,
                args: encoded_args().map_err(|error| format!("{error:?}"))?,
            })
        } else if let Some((proxy_module, symbol)) = adapter_identity {
            Ok(crate::orchestrator::future_registry::Activation::Adapter {
                proxy_module,
                symbol,
                args: encoded_args().map_err(|error| format!("{error:?}"))?,
                request_id: None,
            })
        } else {
            Ok(
                crate::orchestrator::future_registry::Activation::GalfusFunction {
                    module_id: target_module_id,
                    func_idx,
                    args: args_vm,
                },
            )
        }
    }

    fn provider_surface_contract(
        &self,
        alias: &str,
        operation: &str,
    ) -> Option<galfus_contract::SurfaceFunctionContract> {
        let providers = self.vm.as_ref()?.providers()?;
        let providers = providers.lock().ok()?;
        let host = providers.get_host(alias)?;
        let host = host.lock().ok()?;
        host.descriptor()
            .modules
            .into_iter()
            .find_map(|module| module.surface_contract(operation).cloned())
    }
}
