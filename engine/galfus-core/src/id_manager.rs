use std::marker::PhantomData;
use std::sync::Mutex;

/// Trait for raw ID extraction and instantiation.
pub trait RawId {
    fn new(raw: u32) -> Self;
    fn raw(&self) -> u32;
}

struct IdManagerState {
    next_id: u32,
    free_list: Vec<u32>,
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
                next_id: start_id,
                free_list: Vec::new(),
            }),
            _phantom: PhantomData,
        }
    }

    /// Allocates a new ID, prioritizing recycled IDs. Panics if exhausted.
    pub fn allocate(&self) -> T {
        self.try_allocate().expect("id manager exhausted")
    }

    /// Attempts to allocate a new ID, returning None if exhausted.
    pub fn try_allocate(&self) -> Option<T> {
        let mut state = self.state.lock().expect("id manager lock poisoned");
        if let Some(id) = state.free_list.pop() {
            Some(T::new(id))
        } else {
            let id = state.next_id;
            state.next_id = state.next_id.checked_add(1)?;
            Some(T::new(id))
        }
    }

    /// Frees an ID so it can be re-allocated.
    pub fn free(&self, id: T) {
        let mut state = self.state.lock().expect("id manager lock poisoned");
        state.free_list.push(id.raw());
    }

    #[doc(hidden)]
    pub fn set_next_id_for_test(&self, next_id: u32) {
        let mut state = self.state.lock().expect("id manager lock poisoned");
        state.next_id = next_id;
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
