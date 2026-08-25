const ITERATIONS = 200_000;
const WORKER_COUNT = 4;

async function main() {
  const started = performance.now();
  const channels = Array.from({ length: WORKER_COUNT }, () => new MessageChannel());
  const workers = Array.from({ length: WORKER_COUNT }, (_, index) => {
    const worker = new Worker(new URL("./tasks_worker.js", import.meta.url));
    worker.postMessage(
      {
        type: "configure",
        inbound: channels[index].port1,
        outbound: channels[(index + 1) % WORKER_COUNT].port2,
      },
      [channels[index].port1, channels[(index + 1) % WORKER_COUNT].port2],
    );
    return worker;
  });
  const result = (await Promise.all(workers.map(runWorker))).reduce((sum, value) => sum + value, 0);
  console.log(`RESULT=${result}`);
  console.log(`TIME_MS=${Math.round(performance.now() - started)}`);
}

function runWorker(worker) {
  return new Promise((resolve, reject) => {
    worker.onmessage = (event) => {
      worker.terminate();
      resolve(event.data);
    };
    worker.onerror = (error) => {
      worker.terminate();
      reject(error);
    };
    worker.postMessage({ type: "run", iterations: ITERATIONS / 20 });
  });
}

await main();
