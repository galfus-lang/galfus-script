use std::fmt;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NameId(u32);

impl NameId {
    pub fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub fn raw(self) -> u32 {
        self.0
    }

    pub fn intern(s: &str) -> Self {
        let mut table = GLOBAL_STRING_TABLE.lock().unwrap();
        Self(table.intern(s))
    }

    pub fn as_str(self) -> &'static str {
        let table = GLOBAL_STRING_TABLE.lock().unwrap();
        table.strings[self.0 as usize]
    }
}

impl fmt::Display for NameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub trait AsNameId {
    fn to_name_id(self) -> NameId;
}

impl AsNameId for NameId {
    fn to_name_id(self) -> NameId {
        self
    }
}

impl AsNameId for &str {
    fn to_name_id(self) -> NameId {
        NameId::intern(self)
    }
}

impl AsNameId for &&str {
    fn to_name_id(self) -> NameId {
        NameId::intern(self)
    }
}

impl AsNameId for &String {
    fn to_name_id(self) -> NameId {
        NameId::intern(self.as_str())
    }
}

impl AsNameId for String {
    fn to_name_id(self) -> NameId {
        NameId::intern(self.as_str())
    }
}

static GLOBAL_STRING_TABLE: LazyLock<Mutex<GlobalStringTable>> =
    LazyLock::new(|| Mutex::new(GlobalStringTable::new()));

#[derive(Debug, Default)]
struct GlobalStringTable {
    strings: Vec<&'static str>,
    lookup: HashMap<&'static str, u32>,
}

impl GlobalStringTable {
    fn new() -> Self {
        Self::default()
    }

    fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.lookup.get(s) {
            return id;
        }
        let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
        let id = self.strings.len() as u32;
        self.strings.push(leaked);
        self.lookup.insert(leaked, id);
        id
    }
}
