const ITERATIONS = 1_000_000;

function matrix4() {
  let a00 = 1,
    a01 = 2,
    a02 = 3,
    a03 = 4;
  let a10 = 5,
    a11 = 6,
    a12 = 7,
    a13 = 8;
  let a20 = 9,
    a21 = 10,
    a22 = 11,
    a23 = 12;
  let a30 = 13,
    a31 = 14,
    a32 = 15,
    a33 = 16;
  let checksum = 0;
  for (let index = 0; index < ITERATIONS; index += 1) {
    const c00 = (a00 * 1 + a01 * 2 + a02 * 3 + a03 * 4) % 1009,
      c01 = (a00 * 2 + a01 * 1 + a02 * 4 + a03 * 3) % 1009,
      c02 = (a00 * 3 + a01 * 4 + a02 * 1 + a03 * 2) % 1009,
      c03 = (a00 * 4 + a01 * 3 + a02 * 2 + a03 * 1) % 1009;
    const c10 = (a10 * 1 + a11 * 2 + a12 * 3 + a13 * 4) % 1009,
      c11 = (a10 * 2 + a11 * 1 + a12 * 4 + a13 * 3) % 1009,
      c12 = (a10 * 3 + a11 * 4 + a12 * 1 + a13 * 2) % 1009,
      c13 = (a10 * 4 + a11 * 3 + a12 * 2 + a13 * 1) % 1009;
    const c20 = (a20 * 1 + a21 * 2 + a22 * 3 + a23 * 4) % 1009,
      c21 = (a20 * 2 + a21 * 1 + a22 * 4 + a23 * 3) % 1009,
      c22 = (a20 * 3 + a21 * 4 + a22 * 1 + a23 * 2) % 1009,
      c23 = (a20 * 4 + a21 * 3 + a22 * 2 + a23 * 1) % 1009;
    const c30 = (a30 * 1 + a31 * 2 + a32 * 3 + a33 * 4) % 1009,
      c31 = (a30 * 2 + a31 * 1 + a32 * 4 + a33 * 3) % 1009,
      c32 = (a30 * 3 + a31 * 4 + a32 * 1 + a33 * 2) % 1009,
      c33 = (a30 * 4 + a31 * 3 + a32 * 2 + a33 * 1) % 1009;
    a00 = c00;
    a01 = c01;
    a02 = c02;
    a03 = c03;
    a10 = c10;
    a11 = c11;
    a12 = c12;
    a13 = c13;
    a20 = c20;
    a21 = c21;
    a22 = c22;
    a23 = c23;
    a30 = c30;
    a31 = c31;
    a32 = c32;
    a33 = c33;
    checksum += a00 + a11 + a22 + a33;
  }
  return checksum;
}

const start = performance.now();
const result = matrix4();
console.log(`RESULT=${result}`);
console.log(`TIME_MS=${Math.round(performance.now() - start)}`);
