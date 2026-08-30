pub(crate) fn round_float_to_int(value: f32) -> i32 {
    (f64::from(value) + 0.5).floor() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_rounding_uses_java_math_round_semantics() {
        assert_eq!(round_float_to_int(8_388_609.0), 8_388_609);
        assert_eq!(round_float_to_int(-8_388_609.0), -8_388_609);
        assert_eq!(round_float_to_int(f32::NAN), 0);
        assert_eq!(round_float_to_int(f32::INFINITY), i32::MAX);
        assert_eq!(round_float_to_int(f32::NEG_INFINITY), i32::MIN);
    }
}
