#[cfg(test)]
mod tests;

use galfus_vm::thread::{MailboxMessage, ThreadState, VirtualThread};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

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
    threads: HashMap<ThreadId, VirtualThread>,
    mailboxes: HashMap<ThreadId, Arc<Mutex<VecDeque<MailboxMessage>>>>,
    keys: HashMap<String, ThreadId>,
    states: HashMap<ThreadId, ThreadState>,
}

impl ThreadRegistry {
    pub fn new() -> Self {
        Self {
            threads: HashMap::new(),
            mailboxes: HashMap::new(),
            keys: HashMap::new(),
            states: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: ThreadId, thread: VirtualThread) {
        self.park(id, thread);
    }

    pub fn register_with_id(&mut self, id: ThreadId, thread: VirtualThread) {
        self.park(id, thread);
    }

    pub fn park(&mut self, id: ThreadId, thread: VirtualThread) {
        let mailbox = thread.mailbox.clone();
        if let Some(key) = &thread.key {
            self.keys.insert(key.clone(), id);
        }
        self.states.insert(id, thread.state);
        self.threads.insert(id, thread);
        self.mailboxes.entry(id).or_insert(mailbox);
    }

    pub fn get_mailbox(&self, id: ThreadId) -> Option<Arc<Mutex<VecDeque<MailboxMessage>>>> {
        self.mailboxes.get(&id).cloned()
    }

    pub fn active_count(&self) -> usize {
        self.states.values().filter(|s| !s.is_exited()).count()
    }

    pub fn get_exit_code(&self, id: ThreadId) -> Option<i32> {
        if let Some(galfus_vm::thread::ThreadState::Exited(code)) = self.states.get(&id) {
            Some(*code)
        } else {
            None
        }
    }

    pub fn debug_states(&self) -> Vec<(ThreadId, ThreadState)> {
        self.states.iter().map(|(&k, &v)| (k, v)).collect()
    }

    pub fn lookup_key(&self, key: &str) -> Option<ThreadId> {
        self.keys.get(key).copied()
    }

    pub fn take(&mut self, id: ThreadId) -> Option<VirtualThread> {
        self.threads.remove(&id)
    }

    pub fn take_created(&mut self, id: ThreadId) -> Option<VirtualThread> {
        (self.state(id) == Some(ThreadState::Created))
            .then(|| self.take(id))
            .flatten()
    }

    pub fn get_mut(&mut self, id: ThreadId) -> Option<&mut VirtualThread> {
        self.threads.get_mut(&id)
    }

    pub fn get(&self, id: ThreadId) -> Option<&VirtualThread> {
        self.threads.get(&id)
    }

    pub fn contains(&self, id: ThreadId) -> bool {
        self.states.contains_key(&id)
    }

    pub fn state(&self, id: ThreadId) -> Option<ThreadState> {
        self.states.get(&id).copied()
    }

    pub fn mark_running(&mut self, id: ThreadId) -> bool {
        let Some(state) = self.states.get_mut(&id) else {
            return false;
        };
        if *state != ThreadState::Created {
            return false;
        }
        *state = ThreadState::Running;
        if let Some(thread) = self.threads.get_mut(&id) {
            let _ = thread.mark_running();
        }
        true
    }

    pub fn mark_exited(&mut self, id: ThreadId, code: i32) -> bool {
        let Some(state) = self.states.get_mut(&id) else {
            return false;
        };
        if !state.is_running() {
            return false;
        }
        *state = ThreadState::Exited(code);
        if let Some(thread) = self.threads.get_mut(&id) {
            let _ = thread.mark_exited(code);
        }
        true
    }

    pub fn remove(&mut self, id: ThreadId) -> Option<VirtualThread> {
        self.take(id)
    }

    pub fn cancel(&mut self, id: ThreadId) -> bool {
        let removed = self.threads.remove(&id).is_some() || self.states.contains_key(&id);
        self.states.remove(&id);
        self.mailboxes.remove(&id);
        self.keys.retain(|_, thread_id| *thread_id != id);
        removed
    }
}

impl Default for ThreadRegistry {
    fn default() -> Self {
        Self::new()
    }
}
