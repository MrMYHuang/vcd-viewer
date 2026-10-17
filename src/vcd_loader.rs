use crate::byte_source::{LineChunkReader, VcdByteSource};
use crate::vcd_header::{VcdHeader, VcdIdCode};
use crate::waveform_db::{parse_vcd_vector_bytes, LogicState, SignalValue, SignalWave};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy)]
pub struct TimeCheckpoint {
    pub byte_offset: u64,
    pub timestamp: u64,
}

#[derive(Debug, Default, Clone)]
pub struct VcdIndex {
    pub checkpoints: Vec<TimeCheckpoint>,
    pub min_timestamp: u64,
    pub max_timestamp: u64,
    pub bytes_indexed: u64,
    pub is_complete: bool,
}

pub struct StreamingBodyScanner<'a> {
    reader: LineChunkReader<'a>,
    header: &'a VcdHeader,
    watched_ids: HashSet<VcdIdCode>,
    id_widths: HashMap<VcdIdCode, u32>,
    chunk_size: usize,
    current_timestamp: u64,
    seen_first_timestamp: bool,
    last_checkpoint_offset: u64,
    pub index: VcdIndex,
    pub waves: HashMap<VcdIdCode, SignalWave>,
}

impl<'a> StreamingBodyScanner<'a> {
    pub fn new(
        source: &'a dyn VcdByteSource,
        header: &'a VcdHeader,
        watched_ids: HashSet<VcdIdCode>,
        chunk_size: usize,
    ) -> Self {
        let mut waves = HashMap::with_capacity(watched_ids.len());
        let mut id_widths = HashMap::with_capacity(watched_ids.len());
        for &vid in &watched_ids {
            let width = header
                .vcd_id_to_signals
                .get(&vid)
                .and_then(|sigs| sigs.first())
                .map(|sid| header.signals[sid.0].width)
                .unwrap_or(1);
            id_widths.insert(vid, width);
            waves.insert(vid, SignalWave::new(width));
        }

        let start = header.body_start_offset;
        Self {
            reader: LineChunkReader::new(source, start, chunk_size.max(64)),
            header,
            watched_ids,
            id_widths,
            chunk_size: chunk_size.max(64),
            current_timestamp: 0,
            seen_first_timestamp: false,
            last_checkpoint_offset: start,
            index: VcdIndex {
                checkpoints: Vec::new(),
                min_timestamp: 0,
                max_timestamp: 0,
                bytes_indexed: start,
                is_complete: false,
            },
            waves,
        }
    }

    pub fn add_watched_ids(&mut self, new_ids: impl IntoIterator<Item = VcdIdCode>) {
        for vid in new_ids {
            if self.watched_ids.insert(vid) {
                let width = self
                    .header
                    .vcd_id_to_signals
                    .get(&vid)
                    .and_then(|sigs| sigs.first())
                    .map(|sid| self.header.signals[sid.0].width)
                    .unwrap_or(1);
                self.id_widths.insert(vid, width);
                self.waves.entry(vid).or_insert_with(|| SignalWave::new(width));
            }
        }
    }

    pub fn step_chunk(&mut self) -> Result<bool, String> {
        if self.index.is_complete {
            return Ok(false);
        }

        let start_offset = self.reader.current_offset();
        let target_offset = start_offset.saturating_add(self.chunk_size as u64);

        while let Some((tok_offset, tok)) = self.reader.next_token().map_err(|e| e.to_string())? {
            if tok.is_empty() {
                continue;
            }
            match tok[0] {
                b'#' => {
                    if let Ok(s) = std::str::from_utf8(&tok[1..]) {
                        if let Ok(ts) = s.parse::<u64>() {
                            self.current_timestamp = ts;
                            if !self.seen_first_timestamp {
                                self.seen_first_timestamp = true;
                                self.index.min_timestamp = ts;
                                self.index.checkpoints.push(TimeCheckpoint {
                                    byte_offset: tok_offset,
                                    timestamp: ts,
                                });
                                self.last_checkpoint_offset = tok_offset;
                            } else if tok_offset.saturating_sub(self.last_checkpoint_offset)
                                >= 16 * 1024 * 1024
                            {
                                self.index.checkpoints.push(TimeCheckpoint {
                                    byte_offset: tok_offset,
                                    timestamp: ts,
                                });
                                self.last_checkpoint_offset = tok_offset;
                            }
                            if ts > self.index.max_timestamp {
                                self.index.max_timestamp = ts;
                            }
                        }
                    }
                }
                b'0' | b'1' | b'x' | b'X' | b'z' | b'Z' => {
                    if tok.len() > 1 {
                        if let Some(vid) = VcdIdCode::from_bytes(&tok[1..]) {
                            if self.watched_ids.contains(&vid) {
                                let state = LogicState::from_byte(tok[0]);
                                let width = self.id_widths.get(&vid).copied().unwrap_or(1);
                                let val = if width <= 1 {
                                    SignalValue::Scalar(state)
                                } else {
                                    parse_vcd_vector_bytes(&tok[0..1], width)
                                };
                                if let Some(w) = self.waves.get_mut(&vid) {
                                    w.push_transition(self.current_timestamp, val);
                                }
                            }
                        }
                    }
                }
                b'b' | b'B' => {
                    let Some((_, id_tok)) = self.reader.next_token().map_err(|e| e.to_string())? else {
                        break;
                    };
                    if let Some(vid) = VcdIdCode::from_bytes(&id_tok) {
                        if self.watched_ids.contains(&vid) {
                            let width = self.id_widths.get(&vid).copied().unwrap_or(1);
                            let val = parse_vcd_vector_bytes(&tok[1..], width);
                            if let Some(w) = self.waves.get_mut(&vid) {
                                w.push_transition(self.current_timestamp, val);
                            }
                        }
                    }
                }
                b'r' | b'R' => {
                    let Some((_, id_tok)) = self.reader.next_token().map_err(|e| e.to_string())? else {
                        break;
                    };
                    if let Some(vid) = VcdIdCode::from_bytes(&id_tok) {
                        if self.watched_ids.contains(&vid) {
                            if let Ok(s) = std::str::from_utf8(&tok[1..]) {
                                if let Ok(r) = s.parse::<f64>() {
                                    if let Some(w) = self.waves.get_mut(&vid) {
                                        w.push_transition(
                                            self.current_timestamp,
                                            SignalValue::Real(r),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                b'$' => {
                    // Directive in body ($dumpvars, $dumpon, $dumpoff, $end, $comment)
                    if tok.as_slice() == b"$comment" {
                        while let Some((_, ctok)) =
                            self.reader.next_token().map_err(|e| e.to_string())?
                        {
                            if ctok.as_slice() == b"$end" {
                                break;
                            }
                        }
                    }
                }
                _ => {}
            }

            if self.reader.current_offset() >= target_offset {
                self.index.bytes_indexed = self.reader.current_offset();
                return Ok(true);
            }
        }

        self.index.bytes_indexed = self.reader.current_offset();
        self.index.is_complete = true;
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::byte_source::MemByteSource;
    use crate::vcd_header::parse_vcd_header;
    use std::collections::HashSet;

    #[test]
    fn test_selective_signal_loading_and_checkpoints() {
        let vcd = b"$timescale 1 ns $end
$scope module top $end
$var wire 1 ! clk $end
$var reg 8 \" data [7:0] $end
$var wire 1 # ignored_sig $end
$upscope $end
$enddefinitions $end
#0
$dumpvars
0!
b00001111 \"
1#
$end
#10
1!
b10100101 \"
0#
#20
0!
";
        let src = MemByteSource::new(vcd.to_vec());
        let hdr = parse_vcd_header(&src).unwrap();
        let clk_vcd_id = hdr.signals[hdr.path_to_signal["top.clk"].0].vcd_id;
        let data_vcd_id = hdr.signals[hdr.path_to_signal["top.data"].0].vcd_id;
        let ignored_vcd_id = hdr.signals[hdr.path_to_signal["top.ignored_sig"].0].vcd_id;

        let watched: HashSet<_> = [clk_vcd_id, data_vcd_id].into_iter().collect();
        let mut scanner = StreamingBodyScanner::new(&src, &hdr, watched, 32);
        while scanner.step_chunk().unwrap() {}

        assert_eq!(scanner.index.max_timestamp, 20);
        assert!(scanner.waves.contains_key(&clk_vcd_id));
        assert!(scanner.waves.contains_key(&data_vcd_id));
        assert!(!scanner.waves.contains_key(&ignored_vcd_id));
        assert_eq!(scanner.waves[&clk_vcd_id].timestamps, vec![0, 10, 20]);
    }

    #[test]
    fn test_load_verilator_example_adder_vcd() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("example/adder.vcd");
        let src = crate::byte_source::FileByteSource::open(&path).unwrap();
        let hdr = parse_vcd_header(&src).unwrap();

        let sum_id = hdr.path_to_signal["tb_adder.sum_o"];
        let dut_sum_id = hdr.path_to_signal["tb_adder.u_dut.sum_o"];
        let sum_vcd_id = hdr.signals[sum_id.0].vcd_id;
        assert_eq!(sum_vcd_id, hdr.signals[dut_sum_id.0].vcd_id);

        let watched: HashSet<_> = [sum_vcd_id].into_iter().collect();
        let mut scanner = StreamingBodyScanner::new(&src, &hdr, watched, 4096);
        while scanner.step_chunk().unwrap() {}

        assert_eq!(scanner.index.max_timestamp, 205_000);
        let sum_wave = &scanner.waves[&sum_vcd_id];
        // At 25ns (25000ps): 15 + 27 = 42
        let (_, val_at_25ns) = sum_wave.value_at(25_000).unwrap();
        assert_eq!(
            crate::format::format_signal_value(
                val_at_25ns,
                crate::format::Radix::Dec,
                crate::format::Signedness::Signed2s
            ),
            "42"
        );
    }
}
