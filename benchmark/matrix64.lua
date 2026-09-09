local iterations = 1000000
local integer_modulus = 1000000007
local float_modulus = 1000000007.0

local function matrix64_i64()
  local a = {1000000000, 1000000001, 1000000002, 1000000003, 1000000004, 1000000005, 1000000006, 1000000007, 1000000008, 1000000009, 1000000010, 1000000011, 1000000012, 1000000013, 1000000014, 1000000015}
  local b = {1000000001, 1000000002, 1000000003, 1000000004, 1000000002, 1000000001, 1000000004, 1000000003, 1000000003, 1000000004, 1000000001, 1000000002, 1000000004, 1000000003, 1000000002, 1000000001}
  local next = {}
  local checksum = 0
  for _ = 1, iterations do
    for row = 0, 3 do
      local offset = row * 4
      for column = 1, 4 do
        next[offset + column] = (a[offset + 1] * b[column] + a[offset + 2] * b[column + 4] + a[offset + 3] * b[column + 8] + a[offset + 4] * b[column + 12]) % integer_modulus
      end
    end
    a, next = next, a
    checksum = (checksum + a[1] + a[6] + a[11] + a[16]) % integer_modulus
  end
  return checksum
end

local function matrix64_f64()
  local a = {1000000000.0, 1000000001.0, 1000000002.0, 1000000003.0, 1000000004.0, 1000000005.0, 1000000006.0, 1000000007.0, 1000000008.0, 1000000009.0, 1000000010.0, 1000000011.0, 1000000012.0, 1000000013.0, 1000000014.0, 1000000015.0}
  local b = {1000000001.0, 1000000002.0, 1000000003.0, 1000000004.0, 1000000002.0, 1000000001.0, 1000000004.0, 1000000003.0, 1000000003.0, 1000000004.0, 1000000001.0, 1000000002.0, 1000000004.0, 1000000003.0, 1000000002.0, 1000000001.0}
  local next = {}
  local checksum = 0.0
  for _ = 1, iterations do
    for row = 0, 3 do
      local offset = row * 4
      for column = 1, 4 do
        next[offset + column] = (a[offset + 1] * b[column] + a[offset + 2] * b[column + 4] + a[offset + 3] * b[column + 8] + a[offset + 4] * b[column + 12]) % float_modulus
      end
    end
    a, next = next, a
    checksum = (checksum + a[1] + a[6] + a[11] + a[16]) % float_modulus
  end
  return checksum
end

local start = os.clock()
local integer_result = matrix64_i64()
local float_result = matrix64_f64()
print("RESULT=" .. integer_result .. "," .. float_result)
print("TIME_MS=" .. math.floor((os.clock() - start) * 1000))
