//! 旧ツール互換の`.mem` / `.prb`シリアライズ。

use crate::assembler::{CellKind, MemoryImage};
use crate::isa::{Address, Word};
use std::{error::Error, fmt};

/// メモリイメージを`@aaa wwwwwwww`形式へ変換する。
pub fn format_mem(image: &MemoryImage) -> String {
    image
        .cells
        .iter()
        .map(|c| format!("@{:03x} {:08x}\n", c.address.get(), c.word))
        .collect()
}
/// データdirectiveだけを16 bit probe形式へ変換する。
///
/// `SYM`と命令は含めず、終端ワード`f0000000`を必ず付加する。
pub fn format_probe(image: &MemoryImage) -> String {
    let mut out = String::new();
    for c in &image.cells {
        if c.kind == CellKind::Data {
            out.push_str(&format!(
                "{:08x}\n",
                ((c.address.get() as u32) << 16) | (c.word & 0xffff)
            ));
        }
    }
    out.push_str("f0000000\n");
    out
}

#[derive(Debug)]
pub struct MemParseError {
    pub line: usize,
    pub message: String,
}
impl fmt::Display for MemParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}
impl Error for MemParseError {}
/// `.mem`テキストをアドレスとワードの組へ復元する。
pub fn parse_mem(text: &str) -> Result<Vec<(Address, Word)>, MemParseError> {
    let mut result = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut p = line.split_whitespace();
        let a = p
            .next()
            .and_then(|x| x.strip_prefix('@'))
            .ok_or_else(|| MemParseError {
                line: i + 1,
                message: "expected @address word".into(),
            })?;
        let w = p.next().ok_or_else(|| MemParseError {
            line: i + 1,
            message: "missing word".into(),
        })?;
        if p.next().is_some() {
            return Err(MemParseError {
                line: i + 1,
                message: "too many fields".into(),
            });
        }
        let av = u16::from_str_radix(a, 16).map_err(|_| MemParseError {
            line: i + 1,
            message: "invalid address".into(),
        })?;
        let addr = Address::new(av).map_err(|e| MemParseError {
            line: i + 1,
            message: e.to_string(),
        })?;
        let word = u32::from_str_radix(w, 16).map_err(|_| MemParseError {
            line: i + 1,
            message: "invalid word".into(),
        })?;
        result.push((addr, word));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembler::{MemoryCell, MemoryImage};
    #[test]
    fn formats() {
        let i = MemoryImage {
            cells: vec![
                MemoryCell {
                    address: Address::new(0x20).unwrap(),
                    word: 0x12345,
                    kind: CellKind::Data,
                },
                MemoryCell {
                    address: Address::new(0x21).unwrap(),
                    word: 1,
                    kind: CellKind::Symbol,
                },
            ],
        };
        assert_eq!(format_mem(&i), "@020 00012345\n@021 00000001\n");
        assert_eq!(format_probe(&i), "00202345\nf0000000\n");
        assert_eq!(parse_mem(&format_mem(&i)).unwrap().len(), 2);
    }
    #[test]
    fn probe_uses_low16_and_excludes_symbols() {
        let image = MemoryImage {
            cells: vec![
                MemoryCell {
                    address: Address::new(0xabc).unwrap(),
                    word: 0x1234_5678,
                    kind: CellKind::Data,
                },
                MemoryCell {
                    address: Address::new(0xdef).unwrap(),
                    word: 0x42,
                    kind: CellKind::Symbol,
                },
            ],
        };

        assert_eq!(format_probe(&image), "0abc5678\nf0000000\n");
    }
}
