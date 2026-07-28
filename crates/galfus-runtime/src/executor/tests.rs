use super::SingleThreadExecutor;
use galfus_contract::ThreadExecutor;

#[test]
fn allocates_monotonic_non_zero_thread_ids() {
    let executor = SingleThreadExecutor::new();

    assert_eq!(executor.allocate_thread_id(), 1);
    assert_eq!(executor.allocate_thread_id(), 2);
    assert_eq!(executor.allocate_thread_id(), 3);
}
