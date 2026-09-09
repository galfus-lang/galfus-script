const ITERATIONS = 1_000_000;
const MODULUS = 1009;

function matrix4I32() {
  let a = new Int32Array([
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
  ]);
  const b = new Int32Array([1, 2, 3, 4, 2, 1, 4, 3, 3, 4, 1, 2, 4, 3, 2, 1]);
  let next = new Int32Array(16);
  let checksum = 0;
  for (let iteration = 0; iteration < ITERATIONS; iteration += 1) {
    for (let row = 0; row < 4; row += 1) {
      const offset = row * 4;
      for (let column = 0; column < 4; column += 1) {
        next[offset + column] =
          (Math.imul(a[offset], b[column]) +
            Math.imul(a[offset + 1], b[column + 4]) +
            Math.imul(a[offset + 2], b[column + 8]) +
            Math.imul(a[offset + 3], b[column + 12])) %
          MODULUS;
      }
    }
    [a, next] = [next, a];
    checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % MODULUS;
  }
  return checksum;
}

function matrix4F32() {
  let a = new Float32Array([
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
  ]);
  const b = new Float32Array([1, 2, 3, 4, 2, 1, 4, 3, 3, 4, 1, 2, 4, 3, 2, 1]);
  let next = new Float32Array(16);
  let checksum = Math.fround(0);
  for (let iteration = 0; iteration < ITERATIONS; iteration += 1) {
    for (let row = 0; row < 4; row += 1) {
      const offset = row * 4;
      for (let column = 0; column < 4; column += 1) {
        let value = Math.fround(a[offset] * b[column]);
        value = Math.fround(value + Math.fround(a[offset + 1] * b[column + 4]));
        value = Math.fround(value + Math.fround(a[offset + 2] * b[column + 8]));
        value = Math.fround(
          value + Math.fround(a[offset + 3] * b[column + 12]),
        );
        next[offset + column] = Math.fround(value % MODULUS);
      }
    }
    [a, next] = [next, a];
    checksum = Math.fround(checksum + a[0]);
    checksum = Math.fround(checksum + a[5]);
    checksum = Math.fround(checksum + a[10]);
    checksum = Math.fround(checksum + a[15]);
    checksum = Math.fround(checksum % MODULUS);
  }
  return checksum;
}

const start = performance.now();
const integerResult = matrix4I32();
const floatResult = matrix4F32();
console.log(`RESULT=${integerResult},${floatResult}`);
console.log(`TIME_MS=${Math.round(performance.now() - start)}`);
