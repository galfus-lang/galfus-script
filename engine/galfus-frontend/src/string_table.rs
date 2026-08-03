use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NameId(pub(crate) u32);

impl NameId {
    pub fn raw(self) -> u32 {
        self.0
    }
}

impl fmt::Display for NameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NameId({})", self.0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct StringTable {
    strings: Vec<Arc<str>>,
    lookup: HashMap<Arc<str>, u32>,
}

impl StringTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, s: &str) -> NameId {
        if let Some(&id) = self.lookup.get(s) {
            return NameId(id);
        }
        let id = self.strings.len() as u32;
        let owned: Arc<str> = Arc::from(s);
        self.strings.push(Arc::clone(&owned));
        self.lookup.insert(owned, id);
        NameId(id)
    }

    pub fn get(&self, s: &str) -> Option<NameId> {
        self.lookup.get(s).map(|&id| NameId(id))
    }

    pub fn resolve(&self, id: NameId) -> Option<&str> {
        self.strings.get(id.0 as usize).map(|s| &**s)
    }
}
