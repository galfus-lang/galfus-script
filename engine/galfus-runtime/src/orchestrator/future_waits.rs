use super::*;

use galfus_contract::BoundaryValue;

#[derive(Clone, Copy)]
pub(crate) struct MailboxFutureWait {
    pub waiting_thread_id: crate::registry::ThreadId,
    pub future_lease: galfus_core::FutureLease,
    pub sender_id: Option<crate::registry::ThreadId>,
    pub deadline_ms: Option<u64>,
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
        let future_lease = galfus_core::FutureLease::new(
            future_id,
            self.future_generations
                .get(&future_id.raw())
                .copied()
                .unwrap_or(0),
        );
        self.mailbox_future_wait_targets
            .insert((thread_id, future_id), (target_thread_id, future_lease));
        self.mailbox_future_waits
            .entry(target_thread_id)
            .or_default()
            .push_back(MailboxFutureWait {
                waiting_thread_id: thread_id,
                future_lease,
                sender_id,
                deadline_ms: timeout_ms.map(|timeout| self.virtual_time_ms.saturating_add(timeout)),
            });
    }

    pub(super) fn complete_mailbox_future_waits(
        &mut self,
        target_thread_id: crate::registry::ThreadId,
    ) {
        loop {
            let Some(wait) = self
                .mailbox_future_waits
                .get(&target_thread_id)
                .and_then(|waits| waits.front().copied())
            else {
                return;
            };
            if !self.mailbox_future_wait_is_registered(target_thread_id, wait) {
                let remove_entry = {
                    let waits = self
                        .mailbox_future_waits
                        .get_mut(&target_thread_id)
                        .expect("mailbox wait is registered");
                    let _ = waits.pop_front();
                    waits.is_empty()
                };
                if remove_entry {
                    self.mailbox_future_waits.remove(&target_thread_id);
                }
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
            let remove_entry = {
                let waits = self
                    .mailbox_future_waits
                    .get_mut(&target_thread_id)
                    .expect("mailbox wait is registered");
                let _ = waits.pop_front();
                waits.is_empty()
            };
            if remove_entry {
                self.mailbox_future_waits.remove(&target_thread_id);
            }
            self.mailbox_future_wait_targets
                .remove(&(wait.waiting_thread_id, wait.future_lease.id));
            self.complete_future(
                wait.waiting_thread_id,
                wait.future_lease.id,
                Ok(BoundaryValue::Bytes(message.data)),
            );
        }
    }

    pub(super) fn expire_mailbox_future_waits(&mut self, delta_ms: u64) {
        self.virtual_time_ms = self.virtual_time_ms.saturating_add(delta_ms);
        let mut expired = Vec::new();
        let current_time_ms = self.virtual_time_ms;
        let mailbox_future_wait_targets = &mut self.mailbox_future_wait_targets;
        self.mailbox_future_waits.retain(|target_thread_id, waits| {
            waits.retain(|wait| {
                let key = (wait.waiting_thread_id, wait.future_lease.id);
                let registered =
                    mailbox_future_wait_targets
                        .get(&key)
                        .is_some_and(|(target, future_lease)| {
                            *target == *target_thread_id && *future_lease == wait.future_lease
                        });
                if !registered {
                    return false;
                }
                if wait
                    .deadline_ms
                    .is_some_and(|deadline| deadline <= current_time_ms)
                {
                    mailbox_future_wait_targets.remove(&key);
                    expired.push(*wait);
                    return false;
                }
                true
            });
            !waits.is_empty()
        });
        for wait in expired {
            self.complete_future(
                wait.waiting_thread_id,
                wait.future_lease.id,
                Ok(BoundaryValue::Null),
            );
        }
    }

    pub(super) fn register_timer_future_wait(
        &mut self,
        thread_id: crate::registry::ThreadId,
        future_id: galfus_core::FutureId,
        timeout_ms: u64,
    ) {
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
            self.complete_future(
                wait.waiting_thread_id,
                wait.future_lease.id,
                Ok(BoundaryValue::Null),
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
}
