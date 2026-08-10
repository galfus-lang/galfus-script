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
    assert_eq!(manager.try_allocate(), Some(TestId(1)));
    assert_eq!(manager.try_allocate(), Some(TestId(2)));
    assert_eq!(manager.try_allocate(), Some(TestId(3)));
}

#[test]
fn reuses_freed_ids() {
    let manager: IdManager<TestId> = IdManager::new(1);
    let _id1 = manager.try_allocate();
    let id2 = manager.try_allocate().unwrap();
    let _id3 = manager.try_allocate();

    manager.free(id2);

    // It should pop the last freed ID
    assert_eq!(manager.try_allocate(), Some(TestId(2)));
    // Then continue sequence
    assert_eq!(manager.try_allocate(), Some(TestId(4)));
}

#[test]
fn ignores_double_free() {
    let manager: IdManager<TestId> = IdManager::new(1);
    let id = manager.try_allocate().unwrap();

    manager.free(TestId(id.raw()));
    manager.free(TestId(id.raw()));

    assert_eq!(manager.try_allocate(), Some(TestId(1)));
    // the duplicate free should not have added a second 1
    assert_eq!(manager.try_allocate(), Some(TestId(2)));
}

#[test]
fn ignores_an_id_that_was_never_allocated() {
    let manager: IdManager<TestId> = IdManager::new(1);

    manager.free(TestId(5));

    assert_eq!(manager.try_allocate(), Some(TestId(1)));
    assert_eq!(manager.try_allocate(), Some(TestId(2)));
}

#[test]
fn allocates_max_u32_once() {
    let manager: IdManager<TestId> = IdManager::new(u32::MAX);
    assert_eq!(manager.try_allocate(), Some(TestId(u32::MAX)));
    assert_eq!(manager.try_allocate(), None);
}
