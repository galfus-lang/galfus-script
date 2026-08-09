# Numerical Semantics and Determinism

Galfus Script is designed to execute deterministically across all supported platforms (Native, Wasm, Embedded). A critical component of this determinism is its strict numerical semantics, particularly concerning floating-point numbers (`f32` and `f64`).

## Canonical Normalization

Floating-point numbers can represent multiple variations of `NaN` (Not-a-Number) with different payloads and sign bits, as well as distinct positive (`+0.0`) and negative (`-0.0`) zeros. To prevent these internal representations from causing observable differences between platforms, Galfus Script enforces a canonical form at all ABI and execution boundaries.

The engine normalizes floats according to the following rules:

1.  **Canonical NaN**: Any `NaN` value (regardless of sign or payload) is forced to a single canonical representation:
    *   `f32`: `0x7FC0_0000`
    *   `f64`: `0x7FF8_0000_0000_0000`
2.  **Zero Normalization**: Negative zero (`-0.0`) is always normalized to positive zero (`+0.0`).

### Normalization Boundaries

Normalization is actively applied at the following boundaries:

*   **Constants & Literals**: Any floating-point literal in source code or bytecode constant is normalized upon loading.
*   **Arithmetic Operations**: The output of all basic floating-point operations (`+`, `-`, `*`, `/`, `**`, unary `-`) is normalized. Float division follows IEEE-754: division by zero yields an infinity or canonical `NaN` rather than a runtime division-by-zero error.
*   **Type Casting**: Conversions from integers to floats or between `f32` and `f64` produce normalized results.
*   **Host Boundary (`BoundaryValue`)**: Values passed between the Host (Rust) and the Virtual Machine are normalized during encoding and decoding.
*   **Codecs**: Boundary codecs normalize every float decoded from host-provided values before it enters VM state.

By enforcing normalization at these boundaries, it becomes impossible for a Galfus Script program to observe a non-canonical `NaN` or `-0.0` from within the virtual machine.

## Mathematical Operations (`std/math`)

While basic operators are bit-exact (after normalization) across IEEE-754 compliant systems, complex mathematical functions (like trigonometric or logarithmic functions) often rely on the underlying standard library or hardware (`libm`), which can vary between OS and architectures (e.g., `x86_64` vs `wasm32`).

Galfus Script provides the `std/math` builtin module, which wraps these complex operations.

### Guarantees and Tolerances

1.  **Basic Operators**: `+`, `-`, `*`, `/` are guaranteed to be bit-exact across platforms, modulo the normalization rules described above.
2.  **Complex Functions**: Functions such as `sin`, `cos`, `tan`, `log`, `sqrt`, `hypot` use the host's standard library implementation. While they do not guarantee bit-exactness between a Native host and a Wasm host, they are guaranteed to be accurate within a platform-acceptable tolerance (typically 1 ULP - Unit in the Last Place). 
3.  **Output Normalization**: The result of any `std/math` function is immediately normalized by the VM, ensuring that `-0.0` or non-canonical `NaN`s produced by the host's math library do not leak into the VM state.

### Equality and Comparisons

*   Equality (`==`): Follows standard IEEE-754 semantics. `NaN != NaN`.
*   Ordering (`<`, `>`, `<=`, `>=`): Any comparison involving `NaN` evaluates to `false`.
*   Hashing and Serialization: Because values are normalized, the bitwise representation is consistent, ensuring stable hashing and byte-for-byte identical serialization across targets.

## Versioning

The numerical semantics are versioned independently within the package image as `NumericSemanticsVersion`. This allows the engine to evolve its math implementations or normalization rules in the future without silently breaking determinism for older packages. The current version is tracked in `CURRENT_NUMERIC_SEMANTICS_VERSION`.
