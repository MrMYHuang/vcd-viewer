use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogicState {
    Zero,
    One,
    X,
    Z,
}

impl LogicState {
    pub fn from_byte(b: u8) -> Self {
        match b {
            b'0' => LogicState::Zero,
            b'1' => LogicState::One,
            b'z' | b'Z' => LogicState::Z,
            _ => LogicState::X,
        }
    }

    pub fn is_xz(self) -> bool {
        matches!(self, LogicState::X | LogicState::Z)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SignalValue {
    Scalar(LogicState),
    Vector(Vec<LogicState>),
    Real(f64),
}

impl SignalValue {
    pub fn has_xz(&self) -> bool {
        match self {
            SignalValue::Scalar(s) => s.is_xz(),
            SignalValue::Vector(v) => v.iter().any(|s| s.is_xz()),
            SignalValue::Real(r) => !r.is_finite(),
        }
    }

    pub fn to_analog_f64(&self, signed: bool) -> f64 {
        match self {
            SignalValue::Scalar(LogicState::One) => 1.0,
            SignalValue::Scalar(_) => 0.0,
            SignalValue::Real(r) => {
                if r.is_finite() {
                    *r
                } else {
                    0.0
                }
            }
            SignalValue::Vector(bits) => {
                if bits.is_empty() || bits.iter().any(|s| s.is_xz()) {
                    return 0.0;
                }
                if bits.len() <= 64 {
                    let mut u: u64 = 0;
                    for &b in bits {
                        u = (u << 1) | if b == LogicState::One { 1 } else { 0 };
                    }
                    if signed && bits[0] == LogicState::One {
                        let shift = 64 - bits.len();
                        let sign_extended = ((u << shift) as i64) >> shift;
                        sign_extended as f64
                    } else {
                        u as f64
                    }
                } else {
                    let mut acc = 0.0f64;
                    for &b in bits {
                        acc = acc * 2.0 + if b == LogicState::One { 1.0 } else { 0.0 };
                    }
                    acc
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LodSummary {
    pub t_start: u64,
    pub t_end: u64,
    pub transition_count: usize,
    pub first_idx: usize,
    pub last_idx: usize,
    pub min_analog: f64,
    pub max_analog: f64,
    pub has_xz: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SignalWave {
    pub width: u32,
    pub timestamps: Vec<u64>,
    pub values: Vec<SignalValue>,
}

impl SignalWave {
    pub fn new(width: u32) -> Self {
        Self {
            width,
            timestamps: Vec::new(),
            values: Vec::new(),
        }
    }

    pub fn push_transition(&mut self, timestamp: u64, value: SignalValue) {
        if let Some(&last_t) = self.timestamps.last() {
            if timestamp == last_t {
                if self.values.len() >= 2 && self.values[self.values.len() - 2] == value {
                    self.timestamps.pop();
                    self.values.pop();
                } else if let Some(last_v) = self.values.last_mut() {
                    *last_v = value;
                }
                return;
            }
            if timestamp < last_t {
                // Out-of-order guard: keep monotonic
                return;
            }
        }
        if let Some(last_v) = self.values.last() {
            if last_v == &value {
                // Deduplicate identical consecutive values unless first entry
                return;
            }
        }
        self.timestamps.push(timestamp);
        self.values.push(value);
    }

    pub fn value_at(&self, timestamp: u64) -> Option<(u64, &SignalValue)> {
        let idx = self.timestamps.partition_point(|&t| t <= timestamp);
        if idx == 0 {
            None
        } else {
            let i = idx - 1;
            Some((self.timestamps[i], &self.values[i]))
        }
    }

    pub fn bit_value_at(&self, timestamp: u64, bit_offset_from_msb: usize) -> Option<LogicState> {
        let (_, val) = self.value_at(timestamp)?;
        match val {
            SignalValue::Scalar(s) => (bit_offset_from_msb == 0).then_some(*s),
            SignalValue::Vector(v) => v.get(bit_offset_from_msb).copied(),
            SignalValue::Real(_) => None,
        }
    }

    pub fn prev_transition(&self, cursor: u64) -> Option<u64> {
        let idx = self.timestamps.partition_point(|&t| t < cursor);
        if idx == 0 {
            None
        } else {
            Some(self.timestamps[idx - 1])
        }
    }

    pub fn next_transition(&self, cursor: u64) -> Option<u64> {
        let idx = self.timestamps.partition_point(|&t| t <= cursor);
        self.timestamps.get(idx).copied()
    }

    pub fn query_viewport_lod(
        &self,
        t_start: u64,
        t_end: u64,
        max_buckets: usize,
        signed: bool,
    ) -> Vec<LodSummary> {
        if self.timestamps.is_empty() || max_buckets == 0 || t_end < t_start {
            return Vec::new();
        }

        let start_idx = self
            .timestamps
            .partition_point(|&t| t <= t_start)
            .saturating_sub(1);
        let end_idx = self
            .timestamps
            .partition_point(|&t| t <= t_end)
            .min(self.timestamps.len());

        if start_idx >= end_idx {
            return Vec::new();
        }

        let count = end_idx - start_idx;
        if count <= max_buckets {
            let mut out = Vec::with_capacity(count);
            for i in start_idx..end_idx {
                let t0 = self.timestamps[i];
                let t1 = self.timestamps.get(i + 1).copied().unwrap_or(t_end.max(t0));
                let a = self.values[i].to_analog_f64(signed);
                out.push(LodSummary {
                    t_start: t0,
                    t_end: t1,
                    transition_count: 1,
                    first_idx: i,
                    last_idx: i,
                    min_analog: a,
                    max_analog: a,
                    has_xz: self.values[i].has_xz(),
                });
            }
            return out;
        }

        // Partition index range into max_buckets groups for O(max_buckets) decimation
        let mut out = Vec::with_capacity(max_buckets);
        for b in 0..max_buckets {
            let b_start = start_idx + (b * count) / max_buckets;
            let b_end = start_idx + ((b + 1) * count) / max_buckets;
            if b_start >= b_end {
                continue;
            }
            let first_idx = b_start;
            let last_idx = b_end - 1;
            let t0 = self.timestamps[first_idx];
            let t1 = self
                .timestamps
                .get(last_idx + 1)
                .copied()
                .unwrap_or(t_end.max(self.timestamps[last_idx]));

            let span = b_end - b_start;
            let step = (span / 16).max(1);
            let mut min_a = f64::INFINITY;
            let mut max_a = f64::NEG_INFINITY;
            let mut has_xz = false;

            let mut idx = b_start;
            while idx < b_end {
                let v = &self.values[idx];
                let a = v.to_analog_f64(signed);
                if a < min_a {
                    min_a = a;
                }
                if a > max_a {
                    max_a = a;
                }
                if v.has_xz() {
                    has_xz = true;
                }
                idx += step;
            }
            // Always include last_idx
            let last_v = &self.values[last_idx];
            let last_a = last_v.to_analog_f64(signed);
            min_a = min_a.min(last_a);
            max_a = max_a.max(last_a);
            has_xz |= last_v.has_xz();

            out.push(LodSummary {
                t_start: t0,
                t_end: t1,
                transition_count: span,
                first_idx,
                last_idx,
                min_analog: min_a,
                max_analog: max_a,
                has_xz,
            });
        }

        out
    }
}

pub fn parse_vcd_vector_bytes(raw_bits: &[u8], width: u32) -> SignalValue {
    let w = width.max(1) as usize;
    if raw_bits.is_empty() {
        return if w == 1 {
            SignalValue::Scalar(LogicState::X)
        } else {
            SignalValue::Vector(vec![LogicState::X; w])
        };
    }

    // IEEE 1364 left-extension rule:
    // If MSB of provided bits is '0' or '1', pad left with '0'.
    // If MSB is 'x'/'X', pad left with 'X'.
    // If MSB is 'z'/'Z', pad left with 'Z'.
    let pad_state = match raw_bits[0] {
        b'0' | b'1' => LogicState::Zero,
        b'z' | b'Z' => LogicState::Z,
        _ => LogicState::X,
    };

    if w == 1 {
        let last = *raw_bits.last().unwrap();
        return SignalValue::Scalar(LogicState::from_byte(last));
    }

    let mut out = Vec::with_capacity(w);
    if raw_bits.len() >= w {
        let start = raw_bits.len() - w;
        for &b in &raw_bits[start..] {
            out.push(LogicState::from_byte(b));
        }
    } else {
        let pad_len = w - raw_bits.len();
        out.resize(pad_len, pad_state);
        for &b in raw_bits {
            out.push(LogicState::from_byte(b));
        }
    }

    SignalValue::Vector(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ieee1364_vector_left_extension() {
        // 'b1' on 4-bit bus -> 0001
        assert_eq!(
            parse_vcd_vector_bytes(b"1", 4),
            SignalValue::Vector(vec![
                LogicState::Zero,
                LogicState::Zero,
                LogicState::Zero,
                LogicState::One
            ])
        );
        // 'bx' on 4-bit bus -> xxxx
        assert_eq!(
            parse_vcd_vector_bytes(b"x", 4),
            SignalValue::Vector(vec![LogicState::X; 4])
        );
        // 'bz0' on 4-bit bus -> zzz0
        assert_eq!(
            parse_vcd_vector_bytes(b"z0", 4),
            SignalValue::Vector(vec![
                LogicState::Z,
                LogicState::Z,
                LogicState::Z,
                LogicState::Zero
            ])
        );
    }

    #[test]
    fn test_signal_wave_queries_and_lod() {
        let mut wave = SignalWave::new(1);
        for i in 0..1000u64 {
            let st = if i % 2 == 0 {
                LogicState::Zero
            } else {
                LogicState::One
            };
            wave.push_transition(i * 10, SignalValue::Scalar(st));
        }
        assert_eq!(
            wave.value_at(25).unwrap().1,
            &SignalValue::Scalar(LogicState::Zero)
        );
        assert_eq!(
            wave.value_at(35).unwrap().1,
            &SignalValue::Scalar(LogicState::One)
        );
        assert_eq!(wave.prev_transition(35), Some(30));
        assert_eq!(wave.prev_transition(30), Some(20));
        assert_eq!(wave.next_transition(30), Some(40));

        let buckets = wave.query_viewport_lod(0, 10_000, 50, false);
        assert!(buckets.len() <= 50 && !buckets.is_empty());
        assert!(buckets[0].transition_count > 1);
    }

    #[test]
    fn test_same_timestamp_glitch_deduplication() {
        let mut wave = SignalWave::new(1);
        wave.push_transition(0, SignalValue::Scalar(LogicState::Zero));
        wave.push_transition(10, SignalValue::Scalar(LogicState::One));
        // Same timestamp delta-cycle reverts back to Zero:
        wave.push_transition(10, SignalValue::Scalar(LogicState::Zero));
        wave.push_transition(20, SignalValue::Scalar(LogicState::One));

        // Transition at 10 reverted to 0 (same as at 0), so next_transition from 0 must be 20
        assert_eq!(wave.next_transition(0), Some(20));
    }
}
