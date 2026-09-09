def fib(n)
  return n if n <= 1
  fib(n - 1) + fib(n - 2)
end

start_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)
result = fib(35)
end_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)

elapsed_ms = ((end_time - start_time) * 1000).to_i
puts "RESULT=#{result}"
puts "TIME_MS=#{elapsed_ms}"
