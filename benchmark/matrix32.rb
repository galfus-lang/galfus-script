ITERATIONS = 1_000_000
MODULUS = 1009

def matrix4_i32
  a = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
  b = [1, 2, 3, 4, 2, 1, 4, 3, 3, 4, 1, 2, 4, 3, 2, 1]
  nxt = Array.new(16, 0)
  checksum = 0
  
  ITERATIONS.times do
    (0..3).each do |row|
      offset = row * 4
      (0..3).each do |column|
        nxt[offset + column] = (a[offset] * b[column] + a[offset + 1] * b[column + 4] + a[offset + 2] * b[column + 8] + a[offset + 3] * b[column + 12]) % MODULUS
      end
    end
    a, nxt = nxt, a
    checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % MODULUS
  end
  checksum
end

def matrix4_f32
  a = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0]
  b = [1.0, 2.0, 3.0, 4.0, 2.0, 1.0, 4.0, 3.0, 3.0, 4.0, 1.0, 2.0, 4.0, 3.0, 2.0, 1.0]
  nxt = Array.new(16, 0.0)
  checksum = 0.0
  
  ITERATIONS.times do
    (0..3).each do |row|
      offset = row * 4
      (0..3).each do |column|
        nxt[offset + column] = (a[offset] * b[column] + a[offset + 1] * b[column + 4] + a[offset + 2] * b[column + 8] + a[offset + 3] * b[column + 12]) % MODULUS
      end
    end
    a, nxt = nxt, a
    checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % MODULUS
  end
  checksum.to_i
end

start_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)
integer_result = matrix4_i32()
float_result = matrix4_f32()

puts "RESULT=#{integer_result},#{float_result}"
puts "TIME_MS=#{((Process.clock_gettime(Process::CLOCK_MONOTONIC) - start_time) * 1000).to_i}"
