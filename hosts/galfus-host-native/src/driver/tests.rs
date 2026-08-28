use super::*;

use galfus_contract::{BoundaryValue, RunnableTask};
use std::time::{Duration, Instant};

struct BlockingTask {
    release: Receiver<()>,
}

impl RunnableTask for BlockingTask {
    fn run(self: Box<Self>, _budget: usize) -> ThreadResult {
        self.release.recv().unwrap();
        ThreadResult::Completed(Ok(BoundaryValue::I32(0)))
    }
}

#[test]
fn worker_pool_does_not_exceed_its_configured_capacity() {
    let driver = NativeDriver::with_max_workers(2);
    let (release_tx, release_rx) = unbounded();

    for _ in 0..3 {
        driver.dispatch(KernelTask::Any(Box::new(BlockingTask {
            release: release_rx.clone(),
        })));
    }

    let deadline = Instant::now() + Duration::from_secs(1);
    while driver.worker_count.load(Ordering::Acquire) < 2 && Instant::now() < deadline {
        std::thread::yield_now();
    }

    assert_eq!(driver.worker_count.load(Ordering::Acquire), 2);

    for _ in 0..3 {
        release_tx.send(()).unwrap();
    }
}
