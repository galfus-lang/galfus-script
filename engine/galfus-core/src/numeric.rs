#[cfg(test)]
mod tests;

pub const CANONICAL_F32_NAN: u32 = 0x7FC0_0000;
pub const CANONICAL_F64_NAN: u64 = 0x7FF8_0000_0000_0000;

pub fn normalize_f32(value: f32) -> f32 {
    if value.is_nan() {
        f32::from_bits(CANONICAL_F32_NAN)
    } else if value == 0.0 && value.is_sign_negative() {
        0.0 // +0.0
    } else {
        value
    }
}

pub fn normalize_f64(value: f64) -> f64 {
    if value.is_nan() {
        f64::from_bits(CANONICAL_F64_NAN)
    } else if value == 0.0 && value.is_sign_negative() {
        0.0 // +0.0
    } else {
        value
    }
}
