local function fib(n)
  if n <= 1 then
    return n
  end
  return fib(n - 1) + fib(n - 2)
end

local start = os.clock()
local result = fib(35)
local end_time = os.clock()

local elapsed_ms = math.floor((end_time - start) * 1000)
print("RESULT=" .. result)
print("TIME_MS=" .. elapsed_ms)
