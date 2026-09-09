public final class Matrix32 {
    private static final int ITERATIONS = 1_000_000;
    private static final int MODULUS = 1_009;

    private Matrix32() {}

    private static int matrix4I32() {
        int[] a = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16};
        int[] b = {1, 2, 3, 4, 2, 1, 4, 3, 3, 4, 1, 2, 4, 3, 2, 1};
        int[] next = new int[16];
        int checksum = 0;
        for (int iteration = 0; iteration < ITERATIONS; iteration += 1) {
            for (int row = 0; row < 4; row += 1) {
                int offset = row * 4;
                for (int column = 0; column < 4; column += 1) {
                    next[offset + column] = (a[offset] * b[column] + a[offset + 1] * b[column + 4] + a[offset + 2] * b[column + 8] + a[offset + 3] * b[column + 12]) % MODULUS;
                }
            }
            int[] swap = a;
            a = next;
            next = swap;
            checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % MODULUS;
        }
        return checksum;
    }

    private static float matrix4F32() {
        float[] a = {1f, 2f, 3f, 4f, 5f, 6f, 7f, 8f, 9f, 10f, 11f, 12f, 13f, 14f, 15f, 16f};
        float[] b = {1f, 2f, 3f, 4f, 2f, 1f, 4f, 3f, 3f, 4f, 1f, 2f, 4f, 3f, 2f, 1f};
        float[] next = new float[16];
        float checksum = 0f;
        for (int iteration = 0; iteration < ITERATIONS; iteration += 1) {
            for (int row = 0; row < 4; row += 1) {
                int offset = row * 4;
                for (int column = 0; column < 4; column += 1) {
                    next[offset + column] = (a[offset] * b[column] + a[offset + 1] * b[column + 4] + a[offset + 2] * b[column + 8] + a[offset + 3] * b[column + 12]) % MODULUS;
                }
            }
            float[] swap = a;
            a = next;
            next = swap;
            checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % MODULUS;
        }
        return checksum;
    }

    public static void main(String[] args) {
        long started = System.nanoTime();
        int integerResult = matrix4I32();
        float floatResult = matrix4F32();
        long elapsedMilliseconds = (System.nanoTime() - started) / 1_000_000L;
        System.out.println("RESULT=" + integerResult + "," + floatResult);
        System.out.println("TIME_MS=" + elapsedMilliseconds);
    }
}
