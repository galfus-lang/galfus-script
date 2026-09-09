const ITERATIONS = 1_000_000;
const INTEGER_MODULUS = 1_000_000_007n;
const FLOAT_MODULUS = 1_000_000_007;

function matrix64I64() {
  let a = new BigInt64Array([
    1_000_000_000n,
    1_000_000_001n,
    1_000_000_002n,
    1_000_000_003n,
    1_000_000_004n,
    1_000_000_005n,
    1_000_000_006n,
    1_000_000_007n,
    1_000_000_008n,
    1_000_000_009n,
    1_000_000_010n,
    1_000_000_011n,
    1_000_000_012n,
    1_000_000_013n,
    1_000_000_014n,
    1_000_000_015n,
  ]);
  const b = new BigInt64Array([
    1_000_000_001n,
    1_000_000_002n,
    1_000_000_003n,
    1_000_000_004n,
    1_000_000_002n,
    1_000_000_001n,
    1_000_000_004n,
    1_000_000_003n,
    1_000_000_003n,
    1_000_000_004n,
    1_000_000_001n,
    1_000_000_002n,
    1_000_000_004n,
    1_000_000_003n,
    1_000_000_002n,
    1_000_000_001n,
  ]);
  let next = new BigInt64Array(16);
  let checksum = 0n;
  for (let iteration = 0; iteration < ITERATIONS; iteration += 1) {
    for (let row = 0; row < 4; row += 1) {
      const offset = row * 4;
      for (let column = 0; column < 4; column += 1) {
        next[offset + column] =
          (a[offset] * b[column] +
            a[offset + 1] * b[column + 4] +
            a[offset + 2] * b[column + 8] +
            a[offset + 3] * b[column + 12]) %
          INTEGER_MODULUS;
      }
    }
    [a, next] = [next, a];
    checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % INTEGER_MODULUS;
  }
  return checksum;
}

function matrix64F64() {
  let a = new Float64Array([
    1_000_000_000, 1_000_000_001, 1_000_000_002, 1_000_000_003, 1_000_000_004,
    1_000_000_005, 1_000_000_006, 1_000_000_007, 1_000_000_008, 1_000_000_009,
    1_000_000_010, 1_000_000_011, 1_000_000_012, 1_000_000_013, 1_000_000_014,
    1_000_000_015,
  ]);
  const b = new Float64Array([
    1_000_000_001, 1_000_000_002, 1_000_000_003, 1_000_000_004, 1_000_000_002,
    1_000_000_001, 1_000_000_004, 1_000_000_003, 1_000_000_003, 1_000_000_004,
    1_000_000_001, 1_000_000_002, 1_000_000_004, 1_000_000_003, 1_000_000_002,
    1_000_000_001,
  ]);
  let next = new Float64Array(16);
  let checksum = 0;
  for (let iteration = 0; iteration < ITERATIONS; iteration += 1) {
    for (let row = 0; row < 4; row += 1) {
      const offset = row * 4;
      for (let column = 0; column < 4; column += 1) {
        next[offset + column] =
          (a[offset] * b[column] +
            a[offset + 1] * b[column + 4] +
            a[offset + 2] * b[column + 8] +
            a[offset + 3] * b[column + 12]) %
          FLOAT_MODULUS;
      }
    }
    [a, next] = [next, a];
    checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % FLOAT_MODULUS;
  }
  return checksum;
}

const start = performance.now();
const integerResult = matrix64I64();
const floatResult = matrix64F64();
console.log(`RESULT=${integerResult},${floatResult}`);
console.log(`TIME_MS=${Math.round(performance.now() - start)}`);
