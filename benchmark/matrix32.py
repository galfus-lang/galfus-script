import time

ITERATIONS = 1_000_000
MODULUS = 1009

def matrix4_i32():
    a = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
    b = [1, 2, 3, 4, 2, 1, 4, 3, 3, 4, 1, 2, 4, 3, 2, 1]
    nxt = [0] * 16
    checksum = 0
    
    for _ in range(ITERATIONS):
        for row in range(4):
            offset = row * 4
            for column in range(4):
                nxt[offset + column] = (a[offset] * b[column] + a[offset + 1] * b[column + 4] + a[offset + 2] * b[column + 8] + a[offset + 3] * b[column + 12]) % MODULUS
        a, nxt = nxt, a
        checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % MODULUS
    return checksum

def matrix4_f32():
    a = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0]
    b = [1.0, 2.0, 3.0, 4.0, 2.0, 1.0, 4.0, 3.0, 3.0, 4.0, 1.0, 2.0, 4.0, 3.0, 2.0, 1.0]
    nxt = [0.0] * 16
    checksum = 0.0
    
    for _ in range(ITERATIONS):
        for row in range(4):
            offset = row * 4
            for column in range(4):
                nxt[offset + column] = (a[offset] * b[column] + a[offset + 1] * b[column + 4] + a[offset + 2] * b[column + 8] + a[offset + 3] * b[column + 12]) % MODULUS
        a, nxt = nxt, a
        checksum = (checksum + a[0] + a[5] + a[10] + a[15]) % MODULUS
    return int(checksum) if int(checksum) == checksum else checksum

start = time.perf_counter()
integer_result = matrix4_i32()
float_result = matrix4_f32()
end = time.perf_counter()

elapsed_ms = int((end - start) * 1000)
print(f"RESULT={integer_result},{float_result}")
print(f"TIME_MS={elapsed_ms}")
