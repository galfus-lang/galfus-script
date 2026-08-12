use super::*;

use galfus_contract::BoundaryValue;

#[derive(Clone, Copy)]
pub(crate) struct MailboxFutureWait {
    pub waiting_thread_id: crate::registry::ThreadId,
    pub future_lease: galfus_core::FutureLease,
    pub sender_id: Option<crate::registry::ThreadId>,
    pub deadline_ms: Option<u64>,
}

#[derive(Clone, Copy)]
pub(crate) struct TimerFutureWait {
    pub waiting_thread_id: crate::registry::ThreadId,
    pub future_lease: galfus_core::FutureLease,
    pub deadline_ms: u64,
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
        self.mailbox_future_waits
            .entry(target_thread_id)
            .or_default()
            .push(MailboxFutureWait {
                waiting_thread_id: thread_id,
                future_lease: galfus_core::FutureLease::new(
                    future_id,
                    self.future_generations
                        .get(&future_id.raw())
                        .copied()
                        .unwrap_or(0),
                ),
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
                .and_then(|waits| waits.first().copied())
            else {
                return;
            };
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
                waits.remove(0);
                waits.is_empty()
            };
            if remove_entry {
                self.mailbox_future_waits.remove(&target_thread_id);
            }
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
        self.mailbox_future_waits.retain(|_, waits| {
            let mut index = 0;
            while index < waits.len() {
                if waits[index]
                    .deadline_ms
                    .is_some_and(|deadline| deadline <= self.virtual_time_ms)
                {
                    expired.push(waits.remove(index));
                } else {
                    index += 1;
                }
            }
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
        self.timer_future_waits.push(TimerFutureWait {
            waiting_thread_id: thread_id,
            future_lease: galfus_core::FutureLease::new(
                future_id,
                self.future_generations
                    .get(&future_id.raw())
                    .copied()
                    .unwrap_or(0),
            ),
            deadline_ms: self.virtual_time_ms.saturating_add(timeout_ms),
        });
    }

    pub(super) fn expire_timer_future_waits(&mut self, _delta_ms: u64) {
        // We do NOT increment virtual_time_ms here because expire_mailbox_future_waits already did.
        let mut expired = Vec::new();
        let mut index = 0;
        while index < self.timer_future_waits.len() {
            if self.timer_future_waits[index].deadline_ms <= self.virtual_time_ms {
                expired.push(self.timer_future_waits.remove(index));
            } else {
                index += 1;
            }
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
        self.mailbox_future_waits.retain(|_, waits| {
            waits.retain(|wait| {
                wait.waiting_thread_id != owner_thread_id || wait.future_lease.id != future_id
            });
            !waits.is_empty()
        });
    }
}
