use super::*;

use crate::orchestrator::future_registry::Activation;

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
            Activation::GalfusFunction { .. } => {
                unreachable!("GalfusFunction is executed inline and never started in a worker")
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
            Activation::Internal {
                operation,
                module_id: _,

                args,
            } => {
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
}
