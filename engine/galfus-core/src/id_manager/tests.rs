use super::*;

#[derive(Debug, PartialEq)]
struct TestId(u32);

impl RawId for TestId {
    fn new(raw: u32) -> Self {
        Self(raw)
    }
    fn raw(&self) -> u32 {
        self.0
    }
}

#[test]
fn allocates_sequentially_when_no_free_ids() {
    let manager: IdManager<TestId> = IdManager::new(1);
    assert_eq!(manager.allocate(), TestId(1));
    assert_eq!(manager.allocate(), TestId(2));
    assert_eq!(manager.allocate(), TestId(3));
}

#[test]
fn reuses_freed_ids() {
    let manager: IdManager<TestId> = IdManager::new(1);
    let _id1 = manager.allocate();
    let id2 = manager.allocate();
    let _id3 = manager.allocate();

    manager.free(id2);
    
    // It should pop the last freed ID
    assert_eq!(manager.allocate(), TestId(2));
    // Then continue sequence
    assert_eq!(manager.allocate(), TestId(4));
}
