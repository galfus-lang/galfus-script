public final class Fib {
    private Fib() {}

    private static int fib(int value) {
        if (value <= 1) {
            return value;
        }
        return fib(value - 1) + fib(value - 2);
    }

    public static void main(String[] args) {
        long started = System.nanoTime();
        int result = fib(35);
        long elapsedMilliseconds = (System.nanoTime() - started) / 1_000_000L;

        System.out.println("RESULT=" + result);
        System.out.println("TIME_MS=" + elapsedMilliseconds);
    }
}
