#[cfg(test)]
mod tests;

use galfus_vm::thread::VmThreadState;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Created,
    Running,
    Exited(i32),
}

impl ThreadState {
    pub fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }

    pub fn is_exited(self) -> bool {
        matches!(self, Self::Exited(_))
    }

    pub fn exit_reason(self) -> Option<i32> {
        match self {
            Self::Exited(code) => Some(code),
            Self::Created | Self::Running => None,
        }
    }
}

pub struct MailboxMessage {
    pub sender_id: u64,
    pub data: Vec<u8>,
}

pub struct ThreadControlBlock {
    pub id: ThreadId,
    pub state: ThreadState,
    pub mailbox: Arc<Mutex<VecDeque<MailboxMessage>>>,
    pub key: Option<String>,
    pub vm_state: Option<VmThreadState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThreadId(u64);

impl ThreadId {
    pub(crate) fn from_executor(value: u64) -> Option<Self> {
        (value != 0).then_some(Self(value))
    }

    pub(crate) fn from_raw(value: u64) -> Option<Self> {
        Self::from_executor(value)
    }

    pub(crate) fn raw(self) -> u64 {
        self.0
    }
}

pub struct ThreadRegistry {
    tcbs: HashMap<ThreadId, ThreadControlBlock>,
    keys: HashMap<String, ThreadId>,
}

impl ThreadRegistry {
    pub fn new() -> Self {
        Self {
            tcbs: HashMap::new(),
            keys: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: ThreadId, thread: VmThreadState, key: Option<String>) {
        self.park(id, thread, key);
    }

    pub fn park(&mut self, id: ThreadId, thread: VmThreadState, key: Option<String>) {
        if let Some(ref k) = key {
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
        self.tcbs.get(&id).and_then(|tcb| tcb.state.exit_reason())
    }

    pub fn debug_states(&self) -> Vec<(ThreadId, ThreadState)> {
        self.tcbs.iter().map(|(&k, v)| (k, v.state)).collect()
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
        self.tcbs.get(&id).map(|tcb| tcb.state)
    }

    pub fn mark_running(&mut self, id: ThreadId) -> bool {
        if let Some(tcb) = self.tcbs.get_mut(&id) {
            if tcb.state == ThreadState::Created {
                tcb.state = ThreadState::Running;
                return true;
            }
        }
        false
    }

    pub fn mark_exited(&mut self, id: ThreadId, code: i32) -> bool {
        if let Some(tcb) = self.tcbs.get_mut(&id) {
            if tcb.state.is_running() {
                tcb.state = ThreadState::Exited(code);
                return true;
            }
        }
        false
    }

    pub fn restore_vm_state(&mut self, id: ThreadId, state: VmThreadState) {
        if let Some(tcb) = self.tcbs.get_mut(&id) {
            tcb.vm_state = Some(state);
        }
    }

    pub fn cancel(&mut self, id: ThreadId) -> bool {
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
