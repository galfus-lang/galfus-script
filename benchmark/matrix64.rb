ITERATIONS = 1000000
INTEGER_MODULUS = 1000000007
FLOAT_MODULUS = 1000000007

def matrix64_i64
  a = [1000000000, 1000000001, 1000000002, 1000000003, 1000000004, 1000000005, 1000000006, 1000000007, 1000000008, 1000000009, 1000000010, 1000000011, 1000000012, 1000000013, 1000000014, 1000000015]
  b = [1000000001, 1000000002, 1000000003, 1000000004, 1000000002, 1000000001, 1000000004, 1000000003, 1000000003, 1000000004, 1000000001, 1000000002, 1000000004, 1000000003, 1000000002, 1000000001]
  nxt = Array.new(16, 0)
  checksum = 0
  
  ITERATIONS.times do
    (0..3).each do |row|
      offset = row * 4
      (0..3).each do |column|
        nxt[offset + column] = (a[offset] * b[column] + a[offset + 1] * b[column + 4] + a[offset + 2] * b[column + 8] + a[offset + 3] * b[column + 12]) % INTEGER_MODULUS
      end
    end
    a, nxt = nxt, a
    checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % INTEGER_MODULUS
  end
  checksum
end

def matrix64_f64
  a = [1000000000.0, 1000000001.0, 1000000002.0, 1000000003.0, 1000000004.0, 1000000005.0, 1000000006.0, 1000000007.0, 1000000008.0, 1000000009.0, 1000000010.0, 1000000011.0, 1000000012.0, 1000000013.0, 1000000014.0, 1000000015.0]
  b = [1000000001.0, 1000000002.0, 1000000003.0, 1000000004.0, 1000000002.0, 1000000001.0, 1000000004.0, 1000000003.0, 1000000003.0, 1000000004.0, 1000000001.0, 1000000002.0, 1000000004.0, 1000000003.0, 1000000002.0, 1000000001.0]
  nxt = Array.new(16, 0.0)
  checksum = 0.0
  
  ITERATIONS.times do
    (0..3).each do |row|
      offset = row * 4
      (0..3).each do |column|
        nxt[offset + column] = (a[offset] * b[column] + a[offset + 1] * b[column + 4] + a[offset + 2] * b[column + 8] + a[offset + 3] * b[column + 12]) % FLOAT_MODULUS
      end
    end
    a, nxt = nxt, a
    checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % FLOAT_MODULUS
  end
  checksum.to_i
end

start_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)
integer_result = matrix64_i64()
float_result = matrix64_f64()

puts "RESULT=#{integer_result},#{float_result}"
puts "TIME_MS=#{((Process.clock_gettime(Process::CLOCK_MONOTONIC) - start_time) * 1000).to_i}"
