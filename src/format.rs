use crate::waveform_db::{LogicState, SignalValue, SignalWave};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FloatFormat {
    #[default]
    Auto,
    Fp16,
    Bf16,
    Fp32,
    Fp64,
}

impl FloatFormat {
    pub const ALL: [FloatFormat; 5] = [
        FloatFormat::Auto,
        FloatFormat::Fp16,
        FloatFormat::Bf16,
        FloatFormat::Fp32,
        FloatFormat::Fp64,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            FloatFormat::Auto => "Auto Float",
            FloatFormat::Fp16 => "IEEE 754 FP16 (Half)",
            FloatFormat::Bf16 => "IEEE 754 BF16 (BFloat16)",
            FloatFormat::Fp32 => "IEEE 754 FP32 (Single)",
            FloatFormat::Fp64 => "IEEE 754 FP64 (Double)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Radix {
    Bin,
    Oct,
    #[default]
    Hex,
    Dec,
    Ascii,
    Float(FloatFormat),
}

impl Radix {
    pub fn label(&self) -> &'static str {
        match self {
            Radix::Bin => "Bin",
            Radix::Oct => "Oct",
            Radix::Hex => "Hex",
            Radix::Dec => "Dec",
            Radix::Ascii => "ASCII",
            Radix::Float(FloatFormat::Auto) => "Float (Auto)",
            Radix::Float(FloatFormat::Fp16) => "FP16",
            Radix::Float(FloatFormat::Bf16) => "BF16",
            Radix::Float(FloatFormat::Fp32) => "FP32",
            Radix::Float(FloatFormat::Fp64) => "FP64",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Signedness {
    #[default]
    Unsigned,
    Signed2s,
}

fn bits_xz_summary(bits: &[LogicState]) -> Option<&'static str> {
    let mut has_x = false;
    let mut has_z = false;
    for &b in bits {
        match b {
            LogicState::X => has_x = true,
            LogicState::Z => has_z = true,
            _ => {}
        }
    }
    if has_x {
        Some("X")
    } else if has_z {
        Some("Z")
    } else {
        None
    }
}

fn pack_u64_lsb(bits: &[LogicState], max_bits: usize) -> u64 {
    let take = bits.len().min(max_bits);
    let start = bits.len() - take;
    let mut u = 0u64;
    for &b in &bits[start..] {
        u = (u << 1) | if b == LogicState::One { 1 } else { 0 };
    }
    u
}

fn pack_u128(bits: &[LogicState]) -> u128 {
    let take = bits.len().min(128);
    let start = bits.len() - take;
    let mut u = 0u128;
    for &b in &bits[start..] {
        u = (u << 1) | if b == LogicState::One { 1 } else { 0 };
    }
    u
}

pub fn decode_fp16(bits16: u16) -> f64 {
    let sign = if (bits16 >> 15) & 1 != 0 { -1.0 } else { 1.0 };
    let exp = ((bits16 >> 10) & 0x1F) as i32;
    let mant = (bits16 & 0x03FF) as u32;

    if exp == 0x1F {
        if mant == 0 {
            return sign * f64::INFINITY;
        } else {
            return f64::NAN;
        }
    }
    if exp == 0 {
        if mant == 0 {
            return if sign < 0.0 { -0.0 } else { 0.0 };
        }
        // Subnormal: (-1)^s * 2^-14 * (mant / 1024)
        return sign * (mant as f64 / 1024.0) * 2.0f64.powi(-14);
    }
    // Normal: (-1)^s * 2^(exp - 15) * (1 + mant / 1024)
    sign * (1.0 + (mant as f64 / 1024.0)) * 2.0f64.powi(exp - 15)
}

pub fn decode_bf16(bits16: u16) -> f64 {
    f32::from_bits((bits16 as u32) << 16) as f64
}

pub fn format_f64_clean(val: f64) -> String {
    if val.is_nan() {
        return "NaN".to_string();
    }
    if val.is_infinite() {
        return if val.is_sign_positive() {
            "+Inf".to_string()
        } else {
            "-Inf".to_string()
        };
    }
    if val == 0.0 {
        return if val.is_sign_negative() {
            "-0.0".to_string()
        } else {
            "0.0".to_string()
        };
    }
    let abs = val.abs();
    if !(1e-4..1e7).contains(&abs) {
        format!("{:.6e}", val)
    } else {
        let s = format!("{:.6}", val);
        let trimmed = s.trim_end_matches('0');
        if trimmed.ends_with('.') {
            format!("{}0", trimmed)
        } else {
            trimmed.to_string()
        }
    }
}

fn decode_bits_as_float(bits: &[LogicState], fmt: FloatFormat) -> Result<f64, &'static str> {
    if let Some(xz) = bits_xz_summary(bits) {
        return Err(xz);
    }
    let effective = match fmt {
        FloatFormat::Auto => match bits.len() {
            0..=16 => FloatFormat::Fp16,
            17..=32 => FloatFormat::Fp32,
            _ => FloatFormat::Fp64,
        },
        other => other,
    };

    let val = match effective {
        FloatFormat::Fp16 => decode_fp16(pack_u64_lsb(bits, 16) as u16),
        FloatFormat::Bf16 => decode_bf16(pack_u64_lsb(bits, 16) as u16),
        FloatFormat::Fp32 => f32::from_bits(pack_u64_lsb(bits, 32) as u32) as f64,
        FloatFormat::Fp64 | FloatFormat::Auto => f64::from_bits(pack_u64_lsb(bits, 64)),
    };
    Ok(val)
}

fn format_group_digits(bits: &[LogicState], group_size: usize, prefix: &str) -> String {
    let mut out = String::with_capacity(prefix.len() + bits.len() / group_size + 2);
    out.push_str(prefix);
    let rem = bits.len() % group_size;
    let first_chunk_len = if rem == 0 { group_size } else { rem };

    let mut idx = 0;
    while idx < bits.len() {
        let len = if idx == 0 { first_chunk_len } else { group_size };
        let slice = &bits[idx..idx + len];
        let mut has_x = false;
        let mut has_z = false;
        let mut val = 0u8;
        for &b in slice {
            match b {
                LogicState::Zero => val = val << 1,
                LogicState::One => val = (val << 1) | 1,
                LogicState::X => has_x = true,
                LogicState::Z => has_z = true,
            }
        }
        if has_x {
            out.push('X');
        } else if has_z {
            out.push('Z');
        } else {
            let ch = match val {
                0..=9 => (b'0' + val) as char,
                10..=15 => (b'A' + (val - 10)) as char,
                _ => '?',
            };
            out.push(ch);
        }
        idx += len;
    }
    out
}

pub fn format_signal_value(val: &SignalValue, radix: Radix, signedness: Signedness) -> String {
    match val {
        SignalValue::Scalar(s) => match s {
            LogicState::Zero => "0".to_string(),
            LogicState::One => "1".to_string(),
            LogicState::X => "X".to_string(),
            LogicState::Z => "Z".to_string(),
        },
        SignalValue::Real(r) => format_f64_clean(*r),
        SignalValue::Vector(bits) => {
            if bits.is_empty() {
                return "0".to_string();
            }
            match radix {
                Radix::Bin => {
                    let mut s = String::with_capacity(2 + bits.len());
                    s.push_str("0b");
                    for &b in bits {
                        s.push(match b {
                            LogicState::Zero => '0',
                            LogicState::One => '1',
                            LogicState::X => 'x',
                            LogicState::Z => 'z',
                        });
                    }
                    s
                }
                Radix::Oct => format_group_digits(bits, 3, "0o"),
                Radix::Hex => format_group_digits(bits, 4, "0x"),
                Radix::Dec => {
                    if let Some(xz) = bits_xz_summary(bits) {
                        return xz.to_string();
                    }
                    let u = pack_u128(bits);
                    let w = bits.len().min(128);
                    if signedness == Signedness::Signed2s && bits[0] == LogicState::One {
                        if w == 128 {
                            (u as i128).to_string()
                        } else {
                            let shift = 128 - w;
                            let signed_val = ((u << shift) as i128) >> shift;
                            signed_val.to_string()
                        }
                    } else {
                        u.to_string()
                    }
                }
                Radix::Ascii => {
                    if let Some(xz) = bits_xz_summary(bits) {
                        return xz.to_string();
                    }
                    let mut chars = String::new();
                    let rem = bits.len() % 8;
                    let first_len = if rem == 0 { 8 } else { rem };
                    let mut idx = 0;
                    while idx < bits.len() {
                        let len = if idx == 0 { first_len } else { 8 };
                        let byte = pack_u64_lsb(&bits[idx..idx + len], 8) as u8;
                        if (32..=126).contains(&byte) {
                            chars.push(byte as char);
                        } else {
                            chars.push('.');
                        }
                        idx += len;
                    }
                    format!("\"{}\"", chars)
                }
                Radix::Float(fmt) => match decode_bits_as_float(bits, fmt) {
                    Ok(f) => format_f64_clean(f),
                    Err(xz) => xz.to_string(),
                },
            }
        }
    }
}

pub fn signal_value_to_analog(
    val: &SignalValue,
    radix: Radix,
    signedness: Signedness,
) -> Option<f64> {
    match val {
        SignalValue::Scalar(LogicState::Zero) => Some(0.0),
        SignalValue::Scalar(LogicState::One) => Some(1.0),
        SignalValue::Scalar(_) => None,
        SignalValue::Real(r) => r.is_finite().then_some(*r),
        SignalValue::Vector(bits) => {
            if let Radix::Float(fmt) = radix {
                let f = decode_bits_as_float(bits, fmt).ok()?;
                return f.is_finite().then_some(f);
            }
            if bits_xz_summary(bits).is_some() {
                return None;
            }
            Some(val.to_analog_f64(signedness == Signedness::Signed2s))
        }
    }
}

fn normalize_search_token(s: &str) -> String {
    s.trim()
        .replace('_', "")
        .trim_matches('"')
        .to_ascii_lowercase()
}

pub fn find_value_transition(
    wave: &SignalWave,
    start_time: u64,
    forward: bool,
    query: &str,
    radix: Radix,
    signedness: Signedness,
) -> Option<u64> {
    let q = normalize_search_token(query);
    if q.is_empty() || wave.timestamps.is_empty() {
        return None;
    }

    let matches_at = |val: &SignalValue| -> bool {
        let formatted = normalize_search_token(&format_signal_value(val, radix, signedness));
        if formatted == q || formatted.trim_start_matches("0x").trim_start_matches("0b").trim_start_matches("0o") == q {
            return true;
        }
        // Also check hex/dec cross-matching
        let hex_fmt = normalize_search_token(&format_signal_value(val, Radix::Hex, signedness));
        if hex_fmt == q {
            return true;
        }
        if let (Ok(q_f), Some(v_f)) = (q.parse::<f64>(), signal_value_to_analog(val, radix, signedness)) {
            if (q_f - v_f).abs() <= 1e-5 * q_f.abs().max(1.0) {
                return true;
            }
        }
        false
    };

    if forward {
        let start_idx = wave.timestamps.partition_point(|&t| t <= start_time);
        for i in start_idx..wave.timestamps.len() {
            if matches_at(&wave.values[i]) {
                return Some(wave.timestamps[i]);
            }
        }
    } else {
        let end_idx = wave.timestamps.partition_point(|&t| t < start_time);
        for i in (0..end_idx).rev() {
            if matches_at(&wave.values[i]) {
                return Some(wave.timestamps[i]);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::waveform_db::parse_vcd_vector_bytes;

    #[test]
    fn test_radix_and_signed_formatting() {
        // 8-bit 0xFE = 254 unsigned, -2 signed
        let val = parse_vcd_vector_bytes(b"11111110", 8);
        assert_eq!(
            format_signal_value(&val, Radix::Hex, Signedness::Unsigned),
            "0xFE"
        );
        assert_eq!(
            format_signal_value(&val, Radix::Oct, Signedness::Unsigned),
            "0o376"
        );
        assert_eq!(
            format_signal_value(&val, Radix::Bin, Signedness::Unsigned),
            "0b11111110"
        );
        assert_eq!(
            format_signal_value(&val, Radix::Dec, Signedness::Unsigned),
            "254"
        );
        assert_eq!(
            format_signal_value(&val, Radix::Dec, Signedness::Signed2s),
            "-2"
        );
    }

    #[test]
    fn test_ieee754_float_formatting_and_xz_guard() {
        // FP32: 0x40490FDB = 3.1415927
        let pi_bits = parse_vcd_vector_bytes(b"01000000010010010000111111011011", 32);
        let formatted =
            format_signal_value(&pi_bits, Radix::Float(FloatFormat::Fp32), Signedness::Unsigned);
        assert!(formatted.starts_with("3.14159"));

        // BF16: 0x3F80 = 1.0
        let bf16_one = parse_vcd_vector_bytes(b"0011111110000000", 16);
        assert_eq!(
            format_signal_value(&bf16_one, Radix::Float(FloatFormat::Bf16), Signedness::Unsigned),
            "1.0"
        );

        // FP16: 0x3C00 = 1.0, 0x7C00 = +Inf
        let fp16_one = parse_vcd_vector_bytes(b"0011110000000000", 16);
        assert_eq!(
            format_signal_value(&fp16_one, Radix::Float(FloatFormat::Fp16), Signedness::Unsigned),
            "1.0"
        );
        let fp16_inf = parse_vcd_vector_bytes(b"0111110000000000", 16);
        assert_eq!(
            format_signal_value(&fp16_inf, Radix::Float(FloatFormat::Fp16), Signedness::Unsigned),
            "+Inf"
        );

        // Vector with X bit in FP32 mode must return "X", not a bogus number
        let xz_bits = parse_vcd_vector_bytes(b"0100x000010010010000111111011011", 32);
        assert_eq!(
            format_signal_value(&xz_bits, Radix::Float(FloatFormat::Fp32), Signedness::Unsigned),
            "X"
        );
    }
}
