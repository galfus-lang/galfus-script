ITERATIONS = 200_000
WORKER_COUNT = 4

def worker(inbound, outbound, results)
  state = 1
  20.times do
    (ITERATIONS / 20).times do
      state = (state * 127 + 17) % 1000003
    end
    outbound.write("x")
    msg = inbound.read(1)
    state = (state + msg.length) % 1000003
  end
  results.write(state.to_s + "\n")
end

start_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)

pipes = WORKER_COUNT.times.map { IO.pipe }
results_read, results_write = IO.pipe

pids = WORKER_COUNT.times.map do |index|
  in_pipe = pipes[index][0]
  out_pipe = pipes[(index + 1) % WORKER_COUNT][1]
  
  fork do
    pipes.each do |r, w|
      r.close unless r == in_pipe
      w.close unless w == out_pipe
    end
    results_read.close
    worker(in_pipe, out_pipe, results_write)
    results_write.close
  end
end

pipes.each do |r, w|
  r.close
  w.close
end
results_write.close

result = 0
WORKER_COUNT.times do
  result += results_read.gets.to_i
end

pids.each { |pid| Process.wait(pid) }

puts "RESULT=#{result}"
puts "TIME_MS=#{((Process.clock_gettime(Process::CLOCK_MONOTONIC) - start_time) * 1000).to_i}"
