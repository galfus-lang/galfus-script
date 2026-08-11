#[cfg(test)]
mod tests;

use galfus_vm::thread::VmThreadState;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub enum ThreadState {
    Created,
    Running,
    Exited(Result<galfus_contract::BoundaryValue, galfus_contract::ExecutionFailure>),
}

impl ThreadState {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    pub fn is_exited(&self) -> bool {
        matches!(self, Self::Exited(_))
    }

    pub fn exit_reason(
        &self,
    ) -> Option<Result<galfus_contract::BoundaryValue, galfus_contract::ExecutionFailure>> {
        match self {
            Self::Exited(result) => Some(result.clone()),
            Self::Created | Self::Running => None,
        }
    }
}

pub struct MailboxMessage {
    pub sender_id: ThreadId,
    pub data: Vec<u8>,
}

pub struct ThreadControlBlock {
    pub id: ThreadId,
    pub state: ThreadState,
    pub mailbox: Arc<Mutex<VecDeque<MailboxMessage>>>,
    pub key: Option<String>,
    pub vm_state: Option<VmThreadState>,
}

pub use galfus_core::ThreadId;

pub struct ThreadRegistry {
    tcbs: HashMap<ThreadId, ThreadControlBlock>,
    keys: HashMap<String, ThreadId>,
    spawned_since_observation: std::collections::HashSet<ThreadId>,
}

impl ThreadRegistry {
    pub fn new() -> Self {
        Self {
            tcbs: HashMap::new(),
            keys: HashMap::new(),
            spawned_since_observation: std::collections::HashSet::new(),
        }
    }

    pub fn register(
        &mut self,
        id: ThreadId,
        thread: VmThreadState,
        key: Option<String>,
    ) -> Result<(), galfus_contract::ExecutionFailure> {
        self.park(id, thread, key)
    }

    pub fn park(
        &mut self,
        id: ThreadId,
        thread: VmThreadState,
        key: Option<String>,
    ) -> Result<(), galfus_contract::ExecutionFailure> {
        if let Some(ref k) = key {
            if self.keys.contains_key(k) {
                return Err(galfus_contract::ExecutionFailure::new(
                    galfus_contract::ExecutionFailureKind::DuplicateThreadKey,
                    format!("thread key '{k}' is already registered"),
                ));
            }
            self.keys.insert(k.clone(), id);
        }
        self.tcbs.insert(
            id,
            ThreadControlBlock {
                id,
                state: ThreadState::Created,
                mailbox: Arc::new(Mutex::new(VecDeque::new())),
                key,
                vm_state: Some(thread),
            },
        );
        Ok(())
    }

    pub fn key_is_available(&self, key: Option<&str>) -> bool {
        key.is_none_or(|key| !self.keys.contains_key(key))
    }

    pub fn get_mailbox(&self, id: ThreadId) -> Option<Arc<Mutex<VecDeque<MailboxMessage>>>> {
        self.tcbs.get(&id).map(|tcb| tcb.mailbox.clone())
    }

    pub fn active_count(&self) -> usize {
        self.tcbs
            .values()
            .filter(|tcb| !tcb.state.is_exited())
            .count()
    }

    pub fn get_exit_code(&self, id: ThreadId) -> Option<i32> {
        self.tcbs
            .get(&id)
            .and_then(|tcb| tcb.state.exit_reason())
            .and_then(|result| {
                if let Ok(galfus_contract::BoundaryValue::I32(code)) = result {
                    Some(code)
                } else {
                    None
                }
            })
    }

    pub fn debug_states(&self) -> Vec<(ThreadId, ThreadState)> {
        self.tcbs
            .iter()
            .map(|(&k, v)| (k, v.state.clone()))
            .collect()
    }

    pub fn lookup_key(&self, key: &str) -> Option<ThreadId> {
        self.keys.get(key).copied()
    }

    pub fn take(&mut self, id: ThreadId) -> Option<VmThreadState> {
        self.tcbs.get_mut(&id).and_then(|tcb| tcb.vm_state.take())
    }

    pub fn take_created(&mut self, id: ThreadId) -> Option<VmThreadState> {
        if self.state(id) == Some(ThreadState::Created) {
            self.take(id)
        } else {
            None
        }
    }

    pub fn contains(&self, id: ThreadId) -> bool {
        self.tcbs.contains_key(&id)
    }

    pub fn state(&self, id: ThreadId) -> Option<ThreadState> {
        self.tcbs.get(&id).map(|tcb| tcb.state.clone())
    }

    pub fn mark_spawned(&mut self, id: ThreadId) {
        self.spawned_since_observation.insert(id);
    }

    pub fn is_running(&self, id: ThreadId) -> bool {
        self.spawned_since_observation.contains(&id)
            || self.state(id).is_some_and(|state| state.is_running())
    }

    pub fn is_exited(&mut self, id: ThreadId) -> bool {
        self.state(id).is_some_and(|state| state.is_exited())
            && !self.spawned_since_observation.remove(&id)
    }

    pub fn mark_running(&mut self, id: ThreadId) -> bool {
        if let Some(tcb) = self.tcbs.get_mut(&id) {
            if !tcb.state.is_exited() {
                tcb.state = ThreadState::Running;
                return true;
            }
        }
        false
    }

    pub fn mark_exited(
        &mut self,
        id: ThreadId,
        result: Result<galfus_contract::BoundaryValue, galfus_contract::ExecutionFailure>,
    ) -> bool {
        if let Some(tcb) = self.tcbs.get_mut(&id) {
            tcb.state = ThreadState::Exited(result);
            return true;
        }
        false
    }

    pub fn restore_vm_state(&mut self, id: ThreadId, state: VmThreadState) {
        if let Some(tcb) = self.tcbs.get_mut(&id) {
            tcb.vm_state = Some(state);
        }
    }

    pub fn cancel(&mut self, id: ThreadId) -> bool {
        self.spawned_since_observation.remove(&id);
        if let Some(tcb) = self.tcbs.remove(&id) {
            if let Some(key) = tcb.key {
                self.keys.remove(&key);
            }
            true
        } else {
            false
        }
    }
}

impl Default for ThreadRegistry {
    fn default() -> Self {
        Self::new()
    }
}
