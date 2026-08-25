import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;

public final class Tasks {
    private static final int ITERATIONS = 200_000;
    private static final int WORKER_COUNT = 4;

    private Tasks() {}

    private static int worker(BlockingQueue<String> inbound, BlockingQueue<String> outbound)
            throws InterruptedException {
        int state = 1;
        for (int batch = 0; batch < 20; batch += 1) {
            for (int index = 0; index < ITERATIONS / 20; index += 1) {
                state = (state * 127 + 17) % 1000003;
            }
            outbound.put("x");
            state = (state + inbound.take().length()) % 1000003;
        }
        return state;
    }

    public static void main(String[] args) throws Exception {
        long started = System.nanoTime();
        ExecutorService executor = Executors.newFixedThreadPool(WORKER_COUNT);
        int result = 0;
        try {
            List<Future<Integer>> futures = new ArrayList<>(WORKER_COUNT);
            List<BlockingQueue<String>> channels = new ArrayList<>(WORKER_COUNT);
            for (int index = 0; index < WORKER_COUNT; index += 1) {
                channels.add(new LinkedBlockingQueue<>());
            }
            for (int index = 0; index < WORKER_COUNT; index += 1) {
                BlockingQueue<String> inbound = channels.get(index);
                BlockingQueue<String> outbound = channels.get((index + 1) % WORKER_COUNT);
                futures.add(executor.submit(() -> worker(inbound, outbound)));
            }
            for (Future<Integer> future : futures) {
                result += future.get();
            }
        } finally {
            executor.shutdown();
        }
        long elapsedMilliseconds = (System.nanoTime() - started) / 1_000_000L;
        System.out.println("RESULT=" + result);
        System.out.println("TIME_MS=" + elapsedMilliseconds);
    }
}
