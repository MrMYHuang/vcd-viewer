use crate::byte_source::{LineChunkReader, VcdByteSource};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScopeId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SignalId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VcdIdCode(pub u32);

impl VcdIdCode {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }
        let mut acc: u32 = 0;
        for &b in bytes {
            if !(33..=126).contains(&b) {
                return None;
            }
            acc = acc
                .wrapping_mul(94)
                .wrapping_add((b - 32) as u32);
        }
        Some(Self(acc))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TimeUnit {
    Fs,
    Ps,
    #[default]
    Ns,
    Us,
    Ms,
    S,
}

impl TimeUnit {
    pub const ALL: [TimeUnit; 6] = [
        TimeUnit::Fs,
        TimeUnit::Ps,
        TimeUnit::Ns,
        TimeUnit::Us,
        TimeUnit::Ms,
        TimeUnit::S,
    ];

    pub fn femtoseconds(&self) -> f64 {
        match self {
            TimeUnit::Fs => 1.0,
            TimeUnit::Ps => 1e3,
            TimeUnit::Ns => 1e6,
            TimeUnit::Us => 1e9,
            TimeUnit::Ms => 1e12,
            TimeUnit::S => 1e15,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TimeUnit::Fs => "fs",
            TimeUnit::Ps => "ps",
            TimeUnit::Ns => "ns",
            TimeUnit::Us => "us",
            TimeUnit::Ms => "ms",
            TimeUnit::S => "s",
        }
    }

    pub fn from_str_unit(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "fs" => Some(TimeUnit::Fs),
            "ps" => Some(TimeUnit::Ps),
            "ns" => Some(TimeUnit::Ns),
            "us" => Some(TimeUnit::Us),
            "ms" => Some(TimeUnit::Ms),
            "s" => Some(TimeUnit::S),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timescale {
    pub factor: u32,
    pub unit: TimeUnit,
}

impl Default for Timescale {
    fn default() -> Self {
        Self {
            factor: 1,
            unit: TimeUnit::Ns,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VarType {
    Wire,
    Reg,
    Logic,
    Real,
    Integer,
    Parameter,
    Event,
    Other(String),
}

impl VarType {
    pub fn from_token(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "wire" => VarType::Wire,
            "reg" => VarType::Reg,
            "logic" => VarType::Logic,
            "real" => VarType::Real,
            "integer" => VarType::Integer,
            "parameter" => VarType::Parameter,
            "event" => VarType::Event,
            other => VarType::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalMeta {
    pub id: SignalId,
    pub name: String,
    pub full_path: String,
    pub vcd_id: VcdIdCode,
    pub var_type: VarType,
    pub width: u32,
    pub msb: Option<i32>,
    pub lsb: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeNode {
    pub id: ScopeId,
    pub name: String,
    pub full_path: String,
    pub parent: Option<ScopeId>,
    pub children: Vec<ScopeId>,
    pub signals: Vec<SignalId>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VcdHeader {
    pub timescale: Timescale,
    pub scopes: Vec<ScopeNode>,
    pub root_scopes: Vec<ScopeId>,
    pub signals: Vec<SignalMeta>,
    pub path_to_signal: HashMap<String, SignalId>,
    pub vcd_id_to_signals: HashMap<VcdIdCode, Vec<SignalId>>,
    pub body_start_offset: u64,
}

fn parse_name_and_range(raw_tokens: &[String]) -> (String, Option<i32>, Option<i32>) {
    let joined = raw_tokens.join("");
    if let Some(open_idx) = joined.rfind('[') {
        if joined.ends_with(']') && open_idx > 0 {
            let base_name = joined[..open_idx].to_string();
            let inside = &joined[open_idx + 1..joined.len() - 1];
            if let Some((msb_s, lsb_s)) = inside.split_once(':') {
                if let (Ok(msb), Ok(lsb)) = (msb_s.trim().parse::<i32>(), lsb_s.trim().parse::<i32>()) {
                    return (base_name, Some(msb), Some(lsb));
                }
            } else if let Ok(bit) = inside.trim().parse::<i32>() {
                return (base_name, Some(bit), Some(bit));
            }
        }
    }
    (raw_tokens.first().cloned().unwrap_or_default(), None, None)
}

fn read_until_end(reader: &mut LineChunkReader<'_>) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    while let Some((_, tok)) = reader.next_token().map_err(|e| e.to_string())? {
        let s = String::from_utf8_lossy(&tok).into_owned();
        if s == "$end" {
            return Ok(out);
        }
        out.push(s);
    }
    Err("Unexpected EOF before $end in VCD header".to_string())
}

pub fn parse_vcd_header(source: &dyn VcdByteSource) -> Result<VcdHeader, String> {
    let mut reader = LineChunkReader::new(source, 0, 64 * 1024);
    let mut header = VcdHeader::default();
    let mut scope_stack: Vec<ScopeId> = Vec::new();

    while let Some((tok_offset, tok)) = reader.next_token().map_err(|e| e.to_string())? {
        let word = String::from_utf8_lossy(&tok);
        match word.as_ref() {
            "$timescale" => {
                let args = read_until_end(&mut reader)?;
                let joined = args.join("");
                let digits_len = joined.chars().take_while(|c| c.is_ascii_digit()).count();
                if digits_len > 0 {
                    let factor = joined[..digits_len].parse::<u32>().unwrap_or(1);
                    let unit = TimeUnit::from_str_unit(&joined[digits_len..]).unwrap_or(TimeUnit::Ns);
                    header.timescale = Timescale { factor, unit };
                }
            }
            "$scope" => {
                let args = read_until_end(&mut reader)?;
                let scope_name = args.get(1).cloned().unwrap_or_else(|| "unnamed".to_string());
                let parent = scope_stack.last().copied();
                let full_path = if let Some(pid) = parent {
                    format!("{}.{}", header.scopes[pid.0].full_path, scope_name)
                } else {
                    scope_name.clone()
                };
                let new_id = ScopeId(header.scopes.len());
                header.scopes.push(ScopeNode {
                    id: new_id,
                    name: scope_name,
                    full_path,
                    parent,
                    children: Vec::new(),
                    signals: Vec::new(),
                });
                if let Some(pid) = parent {
                    header.scopes[pid.0].children.push(new_id);
                } else {
                    header.root_scopes.push(new_id);
                }
                scope_stack.push(new_id);
            }
            "$upscope" => {
                let _ = read_until_end(&mut reader)?;
                scope_stack.pop();
            }
            "$var" => {
                let args = read_until_end(&mut reader)?;
                // Format: $var <type> <width> <id_code> <ref_name> [range] $end
                if args.len() >= 4 {
                    let var_type = VarType::from_token(&args[0]);
                    let width = args[1].parse::<u32>().unwrap_or(1);
                    let Some(vcd_id) = VcdIdCode::from_bytes(args[2].as_bytes()) else {
                        continue;
                    };
                    let (name, msb, lsb) = parse_name_and_range(&args[3..]);

                    // Ensure we have a scope even for top-level scopeless vars
                    let active_scope = if let Some(&sid) = scope_stack.last() {
                        sid
                    } else {
                        if header.root_scopes.is_empty() {
                            let root_id = ScopeId(header.scopes.len());
                            header.scopes.push(ScopeNode {
                                id: root_id,
                                name: "root".to_string(),
                                full_path: "root".to_string(),
                                parent: None,
                                children: Vec::new(),
                                signals: Vec::new(),
                            });
                            header.root_scopes.push(root_id);
                        }
                        header.root_scopes[0]
                    };

                    let full_path = format!("{}.{}", header.scopes[active_scope.0].full_path, name);
                    let sig_id = SignalId(header.signals.len());
                    header.signals.push(SignalMeta {
                        id: sig_id,
                        name,
                        full_path: full_path.clone(),
                        vcd_id,
                        var_type,
                        width,
                        msb,
                        lsb,
                    });
                    header.scopes[active_scope.0].signals.push(sig_id);
                    header.path_to_signal.insert(full_path, sig_id);
                    header
                        .vcd_id_to_signals
                        .entry(vcd_id)
                        .or_default()
                        .push(sig_id);
                }
            }
            "$enddefinitions" => {
                let _ = read_until_end(&mut reader)?;
                header.body_start_offset = reader.current_offset();
                return Ok(header);
            }
            s if s.starts_with('#') => {
                // Fallback if VCD omitted $enddefinitions
                header.body_start_offset = tok_offset;
                return Ok(header);
            }
            s if s.starts_with('$') => {
                let _ = read_until_end(&mut reader)?;
            }
            _ => {}
        }
    }

    header.body_start_offset = reader.current_offset();
    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::byte_source::MemByteSource;

    #[test]
    fn test_parse_header_scopes_widths_and_aliases() {
        let vcd = b"$date today $end
$timescale 10 ps $end
$scope module top_soc $end
$var wire 1 ! clk $end
$scope module u_cpu $end
$var wire 1 ! clk_alias $end
$var reg 32 \" pc [31:0] $end
$var real 64 # temp_c $end
$upscope $end
$upscope $end
$enddefinitions $end
#0
0!
";
        let src = MemByteSource::new(vcd.to_vec());
        let hdr = parse_vcd_header(&src).unwrap();
        assert_eq!(
            hdr.timescale,
            Timescale {
                factor: 10,
                unit: TimeUnit::Ps
            }
        );
        assert_eq!(hdr.root_scopes.len(), 1);
        assert_eq!(hdr.signals.len(), 4);

        let clk_id = hdr.path_to_signal["top_soc.clk"];
        let alias_id = hdr.path_to_signal["top_soc.u_cpu.clk_alias"];
        assert_eq!(hdr.signals[clk_id.0].vcd_id, hdr.signals[alias_id.0].vcd_id);
        assert_eq!(hdr.vcd_id_to_signals[&hdr.signals[clk_id.0].vcd_id].len(), 2);

        let pc = &hdr.signals[hdr.path_to_signal["top_soc.u_cpu.pc"].0];
        assert_eq!(pc.width, 32);
        assert_eq!((pc.msb, pc.lsb), (Some(31), Some(0)));
        assert!(hdr.body_start_offset > 0 && hdr.body_start_offset < vcd.len() as u64);
    }
}
