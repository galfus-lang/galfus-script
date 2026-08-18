function fib(n) {
  if (n <= 1) return n;
  return fib(n - 1) + fib(n - 2);
}

const start = performance.now();
const result = fib(35);
const end = performance.now();

console.log(`RESULT=${result}`);
console.log(`TIME_MS=${Math.round(end - start)}`);
