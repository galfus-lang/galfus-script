local iterations = 10000

local function matrix4()
  local a00, a01, a02, a03 = 1, 2, 3, 4
  local a10, a11, a12, a13 = 5, 6, 7, 8
  local a20, a21, a22, a23 = 9, 10, 11, 12
  local a30, a31, a32, a33 = 13, 14, 15, 16
  local checksum = 0
  for _ = 1, iterations do
    local c00, c01, c02, c03 = (a00 * 1 + a01 * 2 + a02 * 3 + a03 * 4) % 1009, (a00 * 2 + a01 * 1 + a02 * 4 + a03 * 3) % 1009, (a00 * 3 + a01 * 4 + a02 * 1 + a03 * 2) % 1009, (a00 * 4 + a01 * 3 + a02 * 2 + a03 * 1) % 1009
    local c10, c11, c12, c13 = (a10 * 1 + a11 * 2 + a12 * 3 + a13 * 4) % 1009, (a10 * 2 + a11 * 1 + a12 * 4 + a13 * 3) % 1009, (a10 * 3 + a11 * 4 + a12 * 1 + a13 * 2) % 1009, (a10 * 4 + a11 * 3 + a12 * 2 + a13 * 1) % 1009
    local c20, c21, c22, c23 = (a20 * 1 + a21 * 2 + a22 * 3 + a23 * 4) % 1009, (a20 * 2 + a21 * 1 + a22 * 4 + a23 * 3) % 1009, (a20 * 3 + a21 * 4 + a22 * 1 + a23 * 2) % 1009, (a20 * 4 + a21 * 3 + a22 * 2 + a23 * 1) % 1009
    local c30, c31, c32, c33 = (a30 * 1 + a31 * 2 + a32 * 3 + a33 * 4) % 1009, (a30 * 2 + a31 * 1 + a32 * 4 + a33 * 3) % 1009, (a30 * 3 + a31 * 4 + a32 * 1 + a33 * 2) % 1009, (a30 * 4 + a31 * 3 + a32 * 2 + a33 * 1) % 1009
    a00, a01, a02, a03 = c00, c01, c02, c03
    a10, a11, a12, a13 = c10, c11, c12, c13
    a20, a21, a22, a23 = c20, c21, c22, c23
    a30, a31, a32, a33 = c30, c31, c32, c33
    checksum = checksum + a00 + a11 + a22 + a33
  end
  return checksum
end

local start = os.clock()
local result = matrix4()
print("RESULT=" .. result)
print("TIME_MS=" .. math.floor((os.clock() - start) * 1000))
