use std::collections::HashSet;
use std::marker::PhantomData;
use std::sync::Mutex;

/// Trait for raw ID extraction and instantiation.
pub trait RawId {
    fn new(raw: u32) -> Self;
    fn raw(&self) -> u32;
}

struct IdManagerState {
    next_id: Option<u32>,
    free_list: Vec<u32>,
    active_ids: HashSet<u32>,
}

/// A thread-safe ID generator and recycler.
pub struct IdManager<T> {
    state: Mutex<IdManagerState>,
    _phantom: PhantomData<T>,
}

impl<T: RawId> IdManager<T> {
    /// Creates a new IdManager starting at the given ID.
    pub fn new(start_id: u32) -> Self {
        Self {
            state: Mutex::new(IdManagerState {
                next_id: Some(start_id),
                free_list: Vec::new(),
                active_ids: HashSet::new(),
            }),
            _phantom: PhantomData,
        }
    }

    /// Attempts to allocate a new ID, returning None if exhausted.
    pub fn try_allocate(&self) -> Option<T> {
        let mut state = self.state.lock().expect("id manager lock poisoned");
        if let Some(id) = state.free_list.pop() {
            state.active_ids.insert(id);
            Some(T::new(id))
        } else if let Some(id) = state.next_id {
            state.next_id = id.checked_add(1);
            state.active_ids.insert(id);
            Some(T::new(id))
        } else {
            None
        }
    }

    /// Frees an ID so it can be re-allocated. Ignores duplicates.
    pub fn free(&self, id: T) {
        let mut state = self.state.lock().expect("id manager lock poisoned");
        let raw = id.raw();
        if state.active_ids.remove(&raw) {
            state.free_list.push(raw);
        }
    }

    #[doc(hidden)]
    pub fn set_next_id_for_test(&self, next_id: u32) {
        let mut state = self.state.lock().expect("id manager lock poisoned");
        state.next_id = Some(next_id);
    }
}

impl<T> Default for IdManager<T>
where
    T: RawId,
{
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests;
