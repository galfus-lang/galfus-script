from multiprocessing import Process, Queue
import time

ITERATIONS = 200_000
WORKER_COUNT = 4


def worker(inbound: Queue, outbound: Queue, results: Queue) -> None:
    state = 1
    for _ in range(20):
        for _ in range(ITERATIONS // 20):
            state = (state * 127 + 17) % 1000003
        outbound.put(b"x")
        state = (state + len(inbound.get(timeout=1))) % 1000003
    results.put(state)


if __name__ == "__main__":
    started = time.perf_counter()
    channels = [Queue() for _ in range(WORKER_COUNT)]
    results = Queue()
    workers = [
        Process(target=worker, args=(channels[index], channels[(index + 1) % WORKER_COUNT], results))
        for index in range(WORKER_COUNT)
    ]
    for process in workers:
        process.start()
    result = sum(results.get(timeout=1) for _ in workers)
    for process in workers:
        process.join()
    print(f"RESULT={result}")
    print(f"TIME_MS={int((time.perf_counter() - started) * 1000)}")
