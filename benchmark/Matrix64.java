public final class Matrix64 {
    private static final int ITERATIONS = 1_000_000;
    private static final long INTEGER_MODULUS = 1_000_000_007L;
    private static final double FLOAT_MODULUS = 1_000_000_007d;

    private Matrix64() {}

    private static long matrix64I64() {
        long[] a = {1_000_000_000L, 1_000_000_001L, 1_000_000_002L, 1_000_000_003L, 1_000_000_004L, 1_000_000_005L, 1_000_000_006L, 1_000_000_007L, 1_000_000_008L, 1_000_000_009L, 1_000_000_010L, 1_000_000_011L, 1_000_000_012L, 1_000_000_013L, 1_000_000_014L, 1_000_000_015L};
        long[] b = {1_000_000_001L, 1_000_000_002L, 1_000_000_003L, 1_000_000_004L, 1_000_000_002L, 1_000_000_001L, 1_000_000_004L, 1_000_000_003L, 1_000_000_003L, 1_000_000_004L, 1_000_000_001L, 1_000_000_002L, 1_000_000_004L, 1_000_000_003L, 1_000_000_002L, 1_000_000_001L};
        long[] next = new long[16];
        long checksum = 0L;
        for (int iteration = 0; iteration < ITERATIONS; iteration += 1) {
            for (int row = 0; row < 4; row += 1) {
                int offset = row * 4;
                for (int column = 0; column < 4; column += 1) {
                    next[offset + column] = (a[offset] * b[column] + a[offset + 1] * b[column + 4] + a[offset + 2] * b[column + 8] + a[offset + 3] * b[column + 12]) % INTEGER_MODULUS;
                }
            }
            long[] swap = a;
            a = next;
            next = swap;
            checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % INTEGER_MODULUS;
        }
        return checksum;
    }

    private static double matrix64F64() {
        double[] a = {1_000_000_000d, 1_000_000_001d, 1_000_000_002d, 1_000_000_003d, 1_000_000_004d, 1_000_000_005d, 1_000_000_006d, 1_000_000_007d, 1_000_000_008d, 1_000_000_009d, 1_000_000_010d, 1_000_000_011d, 1_000_000_012d, 1_000_000_013d, 1_000_000_014d, 1_000_000_015d};
        double[] b = {1_000_000_001d, 1_000_000_002d, 1_000_000_003d, 1_000_000_004d, 1_000_000_002d, 1_000_000_001d, 1_000_000_004d, 1_000_000_003d, 1_000_000_003d, 1_000_000_004d, 1_000_000_001d, 1_000_000_002d, 1_000_000_004d, 1_000_000_003d, 1_000_000_002d, 1_000_000_001d};
        double[] next = new double[16];
        double checksum = 0d;
        for (int iteration = 0; iteration < ITERATIONS; iteration += 1) {
            for (int row = 0; row < 4; row += 1) {
                int offset = row * 4;
                for (int column = 0; column < 4; column += 1) {
                    next[offset + column] = (a[offset] * b[column] + a[offset + 1] * b[column + 4] + a[offset + 2] * b[column + 8] + a[offset + 3] * b[column + 12]) % FLOAT_MODULUS;
                }
            }
            double[] swap = a;
            a = next;
            next = swap;
            checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % FLOAT_MODULUS;
        }
        return checksum;
    }

    public static void main(String[] args) {
        long started = System.nanoTime();
        long integerResult = matrix64I64();
        double floatResult = matrix64F64();
        long elapsedMilliseconds = (System.nanoTime() - started) / 1_000_000L;
        System.out.println("RESULT=" + integerResult + "," + floatResult);
        System.out.println("TIME_MS=" + elapsedMilliseconds);
    }
}
