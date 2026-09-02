use super::*;

use crate::event::FutureValue;
use galfus_contract::BoundaryValue;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MailboxFutureWait {
    pub waiting_thread_id: crate::registry::ThreadId,
    pub future_lease: galfus_core::FutureLease,
    pub sender_id: Option<crate::registry::ThreadId>,
    pub sequence: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct MailboxDeadline {
    pub deadline_ms: u64,
    pub target_thread_id: crate::registry::ThreadId,
    pub wait: MailboxFutureWait,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TimerFutureWait {
    pub deadline_ms: u64,
    pub future_lease: galfus_core::FutureLease,
    pub waiting_thread_id: crate::registry::ThreadId,
}

impl Orchestrator {
    pub(super) fn register_thread_exit_future(
        &mut self,
        target_thread_id: crate::registry::ThreadId,
        owner_thread_id: crate::registry::ThreadId,
        future_id: galfus_core::FutureId,
    ) {
        let generation = self
            .future_generations
            .get(&future_id.raw())
            .copied()
            .unwrap_or(0);
        let future_lease = galfus_core::FutureLease::new(future_id, generation);
        self.thread_exit_waits
            .entry(target_thread_id)
            .or_default()
            .push((owner_thread_id, future_lease));
    }

    pub(super) fn register_mailbox_future_wait(
        &mut self,
        thread_id: crate::registry::ThreadId,
        target_thread_id: crate::registry::ThreadId,
        future_id: galfus_core::FutureId,
        sender_id: Option<crate::registry::ThreadId>,
        timeout_ms: Option<u64>,
    ) {
        #[cfg(feature = "metrics")]
        {
            self.future_metrics.mailbox_waits_registered += 1;
        }
        let future_lease = galfus_core::FutureLease::new(
            future_id,
            self.future_generations
                .get(&future_id.raw())
                .copied()
                .unwrap_or(0),
        );
        self.mailbox_future_wait_targets
            .insert((thread_id, future_id), (target_thread_id, future_lease));
        let wait = MailboxFutureWait {
            waiting_thread_id: thread_id,
            future_lease,
            sender_id,
            sequence: self.mailbox_wait_sequence,
        };
        self.mailbox_wait_sequence = self.mailbox_wait_sequence.wrapping_add(1);
        let queues = self
            .mailbox_future_waits
            .entry(target_thread_id)
            .or_default();
        match sender_id {
            Some(sender_id) => queues
                .by_sender
                .entry(sender_id)
                .or_default()
                .push_back(wait),
            None => queues.any_sender.push_back(wait),
        }
        if let Some(timeout) = timeout_ms {
            self.mailbox_deadlines.insert(MailboxDeadline {
                deadline_ms: self.virtual_time_ms.saturating_add(timeout),
                target_thread_id,
                wait,
            });
        }
    }

    pub(super) fn complete_mailbox_future_waits(
        &mut self,
        target_thread_id: crate::registry::ThreadId,
        sender_id: crate::registry::ThreadId,
    ) {
        loop {
            let Some(wait) = self.next_mailbox_wait(target_thread_id, sender_id) else {
                return;
            };
            if !self.mailbox_future_wait_is_registered(target_thread_id, wait) {
                self.pop_mailbox_wait(target_thread_id, wait);
                continue;
            }
            let message = self
                .kernel
                .get_mailbox(target_thread_id)
                .and_then(|mailbox| {
                    let mut mailbox = mailbox.lock().unwrap();
                    let index = wait.sender_id.map_or_else(
                        || (!mailbox.is_empty()).then_some(0),
                        |sender_id| {
                            mailbox
                                .iter()
                                .position(|message| message.sender_id == sender_id)
                        },
                    )?;
                    mailbox.remove(index)
                });
            let Some(message) = message else {
                return;
            };
            self.pop_mailbox_wait(target_thread_id, wait);
            self.mailbox_future_wait_targets
                .remove(&(wait.waiting_thread_id, wait.future_lease.id));
            #[cfg(feature = "metrics")]
            {
                self.future_metrics.mailbox_waits_completed += 1;
            }
            self.complete_future(
                wait.waiting_thread_id,
                wait.future_lease.id,
                Ok(FutureValue::Boundary(BoundaryValue::Bytes(message.data))),
            );
        }
    }

    pub(super) fn expire_mailbox_future_waits(&mut self, delta_ms: u64) {
        self.virtual_time_ms = self.virtual_time_ms.saturating_add(delta_ms);
        let mut expired = Vec::new();
        while let Some(deadline) = self.mailbox_deadlines.first().copied() {
            if deadline.deadline_ms > self.virtual_time_ms {
                break;
            }
            self.mailbox_deadlines.remove(&deadline);
            if self.mailbox_future_wait_is_registered(deadline.target_thread_id, deadline.wait) {
                self.mailbox_future_wait_targets.remove(&(
                    deadline.wait.waiting_thread_id,
                    deadline.wait.future_lease.id,
                ));
                expired.push(deadline.wait);
            }
        }
        for wait in expired {
            #[cfg(feature = "metrics")]
            {
                self.future_metrics.mailbox_waits_timed_out += 1;
            }
            self.complete_future(
                wait.waiting_thread_id,
                wait.future_lease.id,
                Ok(FutureValue::Boundary(BoundaryValue::Null)),
            );
        }
    }

    pub(super) fn register_timer_future_wait(
        &mut self,
        thread_id: crate::registry::ThreadId,
        future_id: galfus_core::FutureId,
        timeout_ms: u64,
    ) {
        #[cfg(feature = "metrics")]
        {
            self.future_metrics.timer_waits_registered += 1;
        }
        self.timer_future_waits.insert(TimerFutureWait {
            deadline_ms: self.virtual_time_ms.saturating_add(timeout_ms),
            future_lease: galfus_core::FutureLease::new(
                future_id,
                self.future_generations
                    .get(&future_id.raw())
                    .copied()
                    .unwrap_or(0),
            ),
            waiting_thread_id: thread_id,
        });
    }

    pub(super) fn expire_timer_future_waits(&mut self, _delta_ms: u64) {
        // We do NOT increment virtual_time_ms here because expire_mailbox_future_waits already did.
        let mut expired = Vec::new();
        while let Some(wait) = self.timer_future_waits.first().copied() {
            if wait.deadline_ms > self.virtual_time_ms {
                break;
            }
            self.timer_future_waits.remove(&wait);
            expired.push(wait);
        }
        for wait in expired {
            #[cfg(feature = "metrics")]
            {
                self.future_metrics.timer_waits_completed += 1;
            }
            self.complete_future(
                wait.waiting_thread_id,
                wait.future_lease.id,
                Ok(FutureValue::Boundary(BoundaryValue::Null)),
            );
        }
    }

    pub(super) fn remove_mailbox_future_wait(
        &mut self,
        owner_thread_id: crate::registry::ThreadId,
        future_id: galfus_core::FutureId,
    ) {
        self.mailbox_future_wait_targets
            .remove(&(owner_thread_id, future_id));
    }

    fn mailbox_future_wait_is_registered(
        &self,
        target_thread_id: crate::registry::ThreadId,
        wait: MailboxFutureWait,
    ) -> bool {
        self.mailbox_future_wait_targets
            .get(&(wait.waiting_thread_id, wait.future_lease.id))
            .is_some_and(|(target, future_lease)| {
                *target == target_thread_id && *future_lease == wait.future_lease
            })
    }

    fn next_mailbox_wait(
        &self,
        target_thread_id: crate::registry::ThreadId,
        sender_id: crate::registry::ThreadId,
    ) -> Option<MailboxFutureWait> {
        let queues = self.mailbox_future_waits.get(&target_thread_id)?;
        match (
            queues.any_sender.front(),
            queues
                .by_sender
                .get(&sender_id)
                .and_then(|waits| waits.front()),
        ) {
            (Some(any), Some(sender)) => Some(if any.sequence <= sender.sequence {
                *any
            } else {
                *sender
            }),
            (Some(wait), None) | (None, Some(wait)) => Some(*wait),
            (None, None) => None,
        }
    }

    fn pop_mailbox_wait(
        &mut self,
        target_thread_id: crate::registry::ThreadId,
        wait: MailboxFutureWait,
    ) {
        let remove_target = {
            let queues = self
                .mailbox_future_waits
                .get_mut(&target_thread_id)
                .expect("mailbox wait queue exists");
            match wait.sender_id {
                Some(sender_id) => {
                    let waits = queues
                        .by_sender
                        .get_mut(&sender_id)
                        .expect("sender wait queue exists");
                    let _ = waits.pop_front();
                    if waits.is_empty() {
                        queues.by_sender.remove(&sender_id);
                    }
                }
                None => {
                    let _ = queues.any_sender.pop_front();
                }
            }
            queues.any_sender.is_empty() && queues.by_sender.is_empty()
        };
        if remove_target {
            self.mailbox_future_waits.remove(&target_thread_id);
        }
    }
}
