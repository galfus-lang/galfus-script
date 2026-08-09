use super::*;

#[test]
fn normalize_f32_handles_zeroes() {
    assert_eq!(normalize_f32(0.0).to_bits(), 0.0f32.to_bits());
    assert_eq!(normalize_f32(-0.0).to_bits(), 0.0f32.to_bits());
    assert_ne!(normalize_f32(-0.0).to_bits(), (-0.0f32).to_bits());
}

#[test]
fn normalize_f64_handles_zeroes() {
    assert_eq!(normalize_f64(0.0).to_bits(), 0.0f64.to_bits());
    assert_eq!(normalize_f64(-0.0).to_bits(), 0.0f64.to_bits());
    assert_ne!(normalize_f64(-0.0).to_bits(), (-0.0f64).to_bits());
}

#[test]
fn normalize_f32_handles_nans() {
    let quiet_nan = f32::from_bits(0x7FC0_0000);
    let signaling_nan = f32::from_bits(0x7FA0_0000);
    let negative_nan = f32::from_bits(0xFFC0_0000);

    assert_eq!(normalize_f32(quiet_nan).to_bits(), CANONICAL_F32_NAN);
    assert_eq!(normalize_f32(signaling_nan).to_bits(), CANONICAL_F32_NAN);
    assert_eq!(normalize_f32(negative_nan).to_bits(), CANONICAL_F32_NAN);
}

#[test]
fn normalize_f64_handles_nans() {
    let quiet_nan = f64::from_bits(0x7FF8_0000_0000_0000);
    let signaling_nan = f64::from_bits(0x7FF4_0000_0000_0000);
    let negative_nan = f64::from_bits(0xFFF8_0000_0000_0000);

    assert_eq!(normalize_f64(quiet_nan).to_bits(), CANONICAL_F64_NAN);
    assert_eq!(normalize_f64(signaling_nan).to_bits(), CANONICAL_F64_NAN);
    assert_eq!(normalize_f64(negative_nan).to_bits(), CANONICAL_F64_NAN);
}

#[test]
fn normal_values_are_preserved() {
    assert_eq!(normalize_f32(1.0).to_bits(), 1.0f32.to_bits());
    assert_eq!(normalize_f32(-1.0).to_bits(), (-1.0f32).to_bits());
    assert_eq!(normalize_f64(1.0).to_bits(), 1.0f64.to_bits());
    assert_eq!(normalize_f64(-1.0).to_bits(), (-1.0f64).to_bits());

    assert_eq!(
        normalize_f32(f32::INFINITY).to_bits(),
        f32::INFINITY.to_bits()
    );
    assert_eq!(
        normalize_f32(f32::NEG_INFINITY).to_bits(),
        f32::NEG_INFINITY.to_bits()
    );
    assert_eq!(
        normalize_f64(f64::INFINITY).to_bits(),
        f64::INFINITY.to_bits()
    );
    assert_eq!(
        normalize_f64(f64::NEG_INFINITY).to_bits(),
        f64::NEG_INFINITY.to_bits()
    );
}
