use super::*;

use crate::orchestrator::pending::{PendingContinuation, PendingKey};
use galfus_contract::{BoundaryValue, ExecutionFailure};

#[derive(Clone, Copy)]
pub(crate) enum AggregateMode {
    All,
    Race,
}

pub(crate) struct AggregateCoordinator {
    pub(crate) mode: AggregateMode,
    pub(crate) future_ids: Vec<galfus_core::FutureId>,
    pub(crate) pending: PendingContinuation,
    pub(crate) results: Vec<Option<Result<BoundaryValue, ExecutionFailure>>>,
    pub(crate) remaining_results: usize,
    pub(crate) winner: Option<(
        EventSequence,
        usize,
        Result<BoundaryValue, ExecutionFailure>,
    )>,
    pub(crate) armed: bool,
}

impl Orchestrator {
    pub(super) fn complete_aggregate_member(
        &mut self,
        coordinator_id: galfus_core::CoordinatorId,
        index: usize,
        result: Result<BoundaryValue, ExecutionFailure>,
    ) {
        let Some(coordinator) = self.aggregate_coordinators.get_mut(&coordinator_id) else {
            return;
        };
        if index >= coordinator.results.len() || coordinator.results[index].is_some() {
            return;
        }
        coordinator.results[index] = Some(result.clone());
        coordinator.remaining_results -= 1;
        if matches!(coordinator.mode, AggregateMode::Race) {
            let sequence = self
                .active_event_sequence
                .expect("aggregate completions are processed by an event");
            let candidate = (sequence, index);
            if coordinator
                .winner
                .as_ref()
                .is_none_or(|(winner_sequence, winner_index, _)| {
                    candidate < (*winner_sequence, *winner_index)
                })
            {
                coordinator.winner = Some((sequence, index, result));
            }
        }
        if !coordinator.armed {
            return;
        }
        self.pending_aggregate_finishes.insert(coordinator_id);
    }

    pub(super) fn finish_aggregate_if_ready(&mut self, coordinator_id: galfus_core::CoordinatorId) {
        let Some(coordinator) = self.aggregate_coordinators.get(&coordinator_id) else {
            return;
        };
        let result = match coordinator.mode {
            AggregateMode::All if coordinator.remaining_results == 0 => {
                let values = coordinator
                    .results
                    .iter()
                    .map(|result| result.as_ref().expect("all results are present").clone())
                    .collect::<Result<Vec<_>, _>>();
                values.map(BoundaryValue::Tuple)
            }
            AggregateMode::Race => match coordinator.winner.clone() {
                Some((_, _, result)) => result,
                None => return,
            },
            AggregateMode::All => return,
        };
        let Some(coordinator) = self.aggregate_coordinators.remove(&coordinator_id) else {
            return;
        };
        self.quota.lock().unwrap().release_pending_states(1);
        self.coordinator_id_manager.free(coordinator_id);
        if matches!(coordinator.mode, AggregateMode::Race) {
            for future_id in coordinator.future_ids {
                let disposition = self
                    .future_registry
                    .discard_for_race(coordinator.pending.thread_id, future_id);
                if let Ok(future_registry::DiscardDisposition::Running(activation)) = disposition {
                    self.cancel_future_activation(
                        coordinator.pending.thread_id,
                        future_id,
                        activation,
                    );
                }
                self.free_future_id(future_id);
            }
        }
        self.resume_pending(
            coordinator.pending.thread_id,
            coordinator.pending,
            result,
            PendingKey::Coordinator(coordinator_id),
        );
    }

    pub(super) fn finish_pending_aggregates(&mut self) {
        let coordinator_ids = std::mem::take(&mut self.pending_aggregate_finishes);
        for coordinator_id in coordinator_ids {
            self.finish_aggregate_if_ready(coordinator_id);
            if self.failure.is_some() {
                return;
            }
        }
    }
}
