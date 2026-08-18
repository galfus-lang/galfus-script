import time

def fib(n):
    if n <= 1:
        return n
    return fib(n - 1) + fib(n - 2)

start = time.perf_counter()
result = fib(35)
end = time.perf_counter()

elapsed_ms = int((end - start) * 1000)
print(f"RESULT={result}")
print(f"TIME_MS={elapsed_ms}")
