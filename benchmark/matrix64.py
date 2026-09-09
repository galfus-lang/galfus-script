import time

ITERATIONS = 1_000_000
INTEGER_MODULUS = 1_000_000_007
FLOAT_MODULUS = 1_000_000_007.0

def matrix64_i64():
    a = [1000000000, 1000000001, 1000000002, 1000000003, 1000000004, 1000000005, 1000000006, 1000000007, 1000000008, 1000000009, 1000000010, 1000000011, 1000000012, 1000000013, 1000000014, 1000000015]
    b = [1000000001, 1000000002, 1000000003, 1000000004, 1000000002, 1000000001, 1000000004, 1000000003, 1000000003, 1000000004, 1000000001, 1000000002, 1000000004, 1000000003, 1000000002, 1000000001]
    nxt = [0] * 16
    checksum = 0
    
    for _ in range(ITERATIONS):
        for row in range(4):
            offset = row * 4
            for column in range(4):
                nxt[offset + column] = (a[offset] * b[column] + a[offset + 1] * b[column + 4] + a[offset + 2] * b[column + 8] + a[offset + 3] * b[column + 12]) % INTEGER_MODULUS
        a, nxt = nxt, a
        checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % INTEGER_MODULUS
    return checksum

def matrix64_f64():
    a = [1000000000.0, 1000000001.0, 1000000002.0, 1000000003.0, 1000000004.0, 1000000005.0, 1000000006.0, 1000000007.0, 1000000008.0, 1000000009.0, 1000000010.0, 1000000011.0, 1000000012.0, 1000000013.0, 1000000014.0, 1000000015.0]
    b = [1000000001.0, 1000000002.0, 1000000003.0, 1000000004.0, 1000000002.0, 1000000001.0, 1000000004.0, 1000000003.0, 1000000003.0, 1000000004.0, 1000000001.0, 1000000002.0, 1000000004.0, 1000000003.0, 1000000002.0, 1000000001.0]
    nxt = [0.0] * 16
    checksum = 0.0
    
    for _ in range(ITERATIONS):
        for row in range(4):
            offset = row * 4
            for column in range(4):
                nxt[offset + column] = (a[offset] * b[column] + a[offset + 1] * b[column + 4] + a[offset + 2] * b[column + 8] + a[offset + 3] * b[column + 12]) % FLOAT_MODULUS
        a, nxt = nxt, a
        checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % FLOAT_MODULUS
    return int(checksum) if int(checksum) == checksum else checksum

start = time.perf_counter()
integer_result = matrix64_i64()
float_result = matrix64_f64()
end = time.perf_counter()

elapsed_ms = int((end - start) * 1000)
print(f"RESULT={integer_result},{float_result}")
print(f"TIME_MS={elapsed_ms}")
