//! Two-pass assembler for EX3 v3.0.

use crate::isa::{
    Address, BranchOp, Immediate16, ImmediateOp, Instruction, MemoryOp, SpRelativeOp, SystemOp,
    Word,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    error::Error,
    fmt,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AsmErrorKind {
    MissingEnd,
    UnexpectedEnd,
    Syntax(String),
    UnknownMnemonic(String),
    DuplicateLabel(String),
    UndefinedSymbol(String),
    AddressOutOfRange(u32),
    ImmediateOutOfRange(i64),
    InvalidNumber(String),
    OverlappingAddress(Address),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsmError {
    pub span: Span,
    pub kind: AsmErrorKind,
}
impl fmt::Display for AsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: ", self.span.line, self.span.column)?;
        match &self.kind {
            AsmErrorKind::MissingEnd => f.write_str("missing END directive"),
            AsmErrorKind::UnexpectedEnd => f.write_str("content after END directive"),
            AsmErrorKind::Syntax(s) => write!(f, "syntax error: {s}"),
            AsmErrorKind::UnknownMnemonic(s) => write!(f, "unknown mnemonic `{s}`"),
            AsmErrorKind::DuplicateLabel(s) => write!(f, "duplicate label `{s}`"),
            AsmErrorKind::UndefinedSymbol(s) => write!(f, "undefined symbol `{s}`"),
            AsmErrorKind::AddressOutOfRange(v) => write!(f, "address out of range: {v:#x}"),
            AsmErrorKind::ImmediateOutOfRange(v) => write!(f, "immediate out of range: {v}"),
            AsmErrorKind::InvalidNumber(s) => write!(f, "invalid number `{s}`"),
            AsmErrorKind::OverlappingAddress(a) => {
                write!(f, "address {a} is emitted more than once")
            }
        }
    }
}
impl Error for AsmError {}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsmErrors(pub Vec<AsmError>);
impl fmt::Display for AsmErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, e) in self.0.iter().enumerate() {
            if i != 0 {
                writeln!(f)?;
            }
            write!(f, "{e}")?;
        }
        Ok(())
    }
}
impl Error for AsmErrors {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellKind {
    Instruction,
    Data,
    Symbol,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryCell {
    pub address: Address,
    pub word: Word,
    pub kind: CellKind,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryImage {
    pub cells: Vec<MemoryCell>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssemblyResult {
    pub image: MemoryImage,
    pub symbols: BTreeMap<String, Address>,
}

#[derive(Clone, Debug)]
enum Unresolved {
    Instruction {
        mnemonic: String,
        operands: Vec<String>,
    },
    Hex(String),
    Dec(String),
    Chr(String),
    Sym(String),
}
impl Unresolved {
    fn words(&self) -> u32 {
        match self {
            Self::Instruction { mnemonic, .. } if mnemonic == "PUSH" || mnemonic == "POP" => 2,
            _ => 1,
        }
    }
}
#[derive(Clone, Debug)]
struct Line {
    line: usize,
    org: Option<String>,
    labels: Vec<String>,
    statement: Option<Unresolved>,
}

#[derive(Clone, Copy, Debug)]
pub struct Assembler;
impl Default for Assembler {
    fn default() -> Self {
        Self::new()
    }
}
impl Assembler {
    pub const fn new() -> Self {
        Self
    }
    pub fn assemble(&self, source: &str) -> Result<AssemblyResult, AsmErrors> {
        let lines = parse(source)?;
        let mut errors = Vec::new();
        let mut symbols = BTreeMap::new();
        let mut lc = 0u32;
        for line in &lines {
            if let Some(org) = &line.org {
                match parse_org(org) {
                    Ok(v) => lc = v as u32,
                    Err(k) => errors.push(err(line.line, k)),
                }
            }
            for label in &line.labels {
                if !valid_identifier(label) || label == "I" {
                    errors.push(err(
                        line.line,
                        AsmErrorKind::Syntax(format!("invalid label `{label}`")),
                    ))
                } else if symbols.contains_key(label) {
                    errors.push(err(line.line, AsmErrorKind::DuplicateLabel(label.clone())))
                } else if lc > 0xffff {
                    errors.push(err(line.line, AsmErrorKind::AddressOutOfRange(lc)))
                } else {
                    symbols.insert(label.clone(), Address::new(lc as u16).unwrap());
                }
            }
            if let Some(s) = &line.statement {
                lc = lc.saturating_add(s.words());
            }
        }
        if !errors.is_empty() {
            return Err(AsmErrors(errors));
        }
        let lookup: HashMap<_, _> = symbols.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        let mut image = MemoryImage::default();
        let mut used = HashSet::new();
        lc = 0;
        for line in &lines {
            if let Some(org) = &line.org {
                lc = parse_org(org).expect("pass 1") as u32;
            }
            let Some(statement) = &line.statement else {
                continue;
            };
            match resolve(statement, &lookup, line.line) {
                Ok(words) => {
                    for (word, kind) in words {
                        if lc > 0xffff {
                            errors.push(err(line.line, AsmErrorKind::AddressOutOfRange(lc)));
                            lc += 1;
                            continue;
                        }
                        let address = Address::new(lc as u16).unwrap();
                        if !used.insert(address) {
                            errors.push(err(line.line, AsmErrorKind::OverlappingAddress(address)));
                            lc += 1;
                            continue;
                        }
                        image.cells.push(MemoryCell {
                            address,
                            word,
                            kind,
                        });
                        lc += 1;
                    }
                }
                Err(e) => {
                    errors.push(e);
                    lc = lc.saturating_add(statement.words());
                }
            }
        }
        if errors.is_empty() {
            Ok(AssemblyResult { image, symbols })
        } else {
            Err(AsmErrors(errors))
        }
    }
}
fn err(line: usize, kind: AsmErrorKind) -> AsmError {
    AsmError {
        span: Span { line, column: 1 },
        kind,
    }
}

fn parse(source: &str) -> Result<Vec<Line>, AsmErrors> {
    let mut result = Vec::new();
    let mut errors = Vec::new();
    let mut ended = false;
    for (index, raw) in source.lines().enumerate() {
        let line_no = index + 1;
        let text = raw.split([';', '/']).next().unwrap_or(raw).trim();
        if text.is_empty() {
            continue;
        }
        if ended {
            errors.push(err(line_no, AsmErrorKind::UnexpectedEnd));
            continue;
        }
        if text.eq_ignore_ascii_case("END") {
            ended = true;
            continue;
        }
        let (mut rest, mut labels) = (text, Vec::new());
        loop {
            let colon = rest.find(':');
            let comma = rest.find(',');
            let split = match (colon, comma) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            };
            let Some(pos) = split else { break };
            let candidate = rest[..pos].trim();
            if candidate.split_whitespace().count() != 1 {
                break;
            }
            labels.push(candidate.to_string());
            rest = rest[pos + 1..].trim_start();
        }
        let tokens: Vec<String> = rest.split_whitespace().map(str::to_string).collect();
        let (org, statement) = if tokens.is_empty() {
            (None, None)
        } else if tokens[0].eq_ignore_ascii_case("ORG") {
            if tokens.len() != 2 {
                errors.push(err(
                    line_no,
                    AsmErrorKind::Syntax("ORG requires one address".into()),
                ));
                (None, None)
            } else {
                (Some(tokens[1].clone()), None)
            }
        } else {
            let name = tokens[0].to_ascii_uppercase();
            let args = tokens[1..].to_vec();
            let statement = match name.as_str() {
                "HEX" if args.len() == 1 => Some(Unresolved::Hex(args[0].clone())),
                "DEC" if args.len() == 1 => Some(Unresolved::Dec(args[0].clone())),
                "CHR" if args.len() == 1 => Some(Unresolved::Chr(args[0].clone())),
                "SYM" if args.len() == 1 => Some(Unresolved::Sym(args[0].clone())),
                "HEX" | "DEC" | "CHR" | "SYM" => {
                    errors.push(err(
                        line_no,
                        AsmErrorKind::Syntax(format!("{name} requires one operand")),
                    ));
                    None
                }
                _ => Some(Unresolved::Instruction {
                    mnemonic: name,
                    operands: args,
                }),
            };
            (None, statement)
        };
        result.push(Line {
            line: line_no,
            org,
            labels,
            statement,
        });
    }
    // END remains accepted but is optional in v3 source.
    if errors.is_empty() {
        Ok(result)
    } else {
        Err(AsmErrors(errors))
    }
}

fn resolve(
    stmt: &Unresolved,
    symbols: &HashMap<&str, Address>,
    line: usize,
) -> Result<Vec<(Word, CellKind)>, AsmError> {
    match stmt {
        Unresolved::Hex(s) => parse_hex_word(s)
            .map(|v| vec![(v, CellKind::Data)])
            .map_err(|k| err(line, k)),
        Unresolved::Dec(s) => s
            .parse::<i32>()
            .map(|v| vec![(v as u32, CellKind::Data)])
            .map_err(|_| err(line, AsmErrorKind::InvalidNumber(s.clone()))),
        Unresolved::Chr(s) => {
            let inner = if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
                &s[1..s.len() - 1]
            } else {
                s
            };
            let mut cs = inner.chars();
            let c = cs.next().filter(|_| cs.next().is_none()).ok_or_else(|| {
                err(
                    line,
                    AsmErrorKind::Syntax("CHR requires one character".into()),
                )
            })?;
            Ok(vec![(c as u32, CellKind::Data)])
        }
        Unresolved::Sym(s) => {
            symbol_or_address(symbols, s, line).map(|a| vec![(a.get() as u32, CellKind::Symbol)])
        }
        Unresolved::Instruction { mnemonic, operands } => {
            resolve_instruction(mnemonic, operands, symbols, line).map(|xs| {
                xs.into_iter()
                    .map(|i| (i.encode(), CellKind::Instruction))
                    .collect()
            })
        }
    }
}

fn resolve_instruction(
    m: &str,
    o: &[String],
    symbols: &HashMap<&str, Address>,
    line: usize,
) -> Result<Vec<Instruction>, AsmError> {
    if m == "PUSH" || m == "POP" {
        if !o.is_empty() {
            return Err(err(
                line,
                AsmErrorKind::Syntax(format!("{m} takes no operands")),
            ));
        }
        let minus = Immediate16::from_raw(0xffff);
        let zero = Immediate16::from_raw(0);
        let plus = Immediate16::from_raw(1);
        return Ok(if m == "PUSH" {
            vec![
                Instruction::Immediate {
                    op: ImmediateOp::Adjsp,
                    value: minus,
                },
                Instruction::SpRelative {
                    op: SpRelativeOp::Stsp,
                    offset: zero,
                },
            ]
        } else {
            vec![
                Instruction::SpRelative {
                    op: SpRelativeOp::Ldsp,
                    offset: zero,
                },
                Instruction::Immediate {
                    op: ImmediateOp::Adjsp,
                    value: plus,
                },
            ]
        });
    }
    if let Ok(op) = m.parse::<SystemOp>() {
        if o.is_empty() {
            return Ok(vec![Instruction::System(op)]);
        }
        return Err(err(
            line,
            AsmErrorKind::Syntax(format!("{m} takes no operands")),
        ));
    }
    if o.len() != 1 && !(o.len() == 2 && o[1] == "I") {
        return Err(err(
            line,
            AsmErrorKind::Syntax(format!("wrong operand count for {m}")),
        ));
    }
    let indirect = o.len() == 2;
    if o.len() == 2 && o[1] != "I" {
        return Err(err(
            line,
            AsmErrorKind::Syntax("indirect marker must be uppercase I".into()),
        ));
    }
    let operand = &o[0];
    if let Ok(op) = m.parse::<BranchOp>() {
        if indirect {
            return Err(err(
                line,
                AsmErrorKind::Syntax("branches cannot be indirect".into()),
            ));
        }
        return Ok(vec![Instruction::Branch {
            op,
            target: symbol_or_address(symbols, operand, line)?,
        }]);
    }
    if let Ok(op) = m.parse::<SpRelativeOp>() {
        if indirect {
            return Err(err(
                line,
                AsmErrorKind::Syntax("SP-relative instruction cannot be indirect".into()),
            ));
        }
        return Ok(vec![Instruction::SpRelative {
            op,
            offset: parse_signed16(operand, line)?,
        }]);
    }
    if matches!(m, "LDHI" | "LDLO" | "ADJSP") {
        if indirect {
            return Err(err(
                line,
                AsmErrorKind::Syntax("immediate instruction cannot be indirect".into()),
            ));
        }
        let op = m.parse::<ImmediateOp>().unwrap();
        let value = if matches!(op, ImmediateOp::Ldhi | ImmediateOp::Ldlo) {
            parse_raw16(operand, line)?
        } else {
            parse_signed16(operand, line)?
        };
        return Ok(vec![Instruction::Immediate { op, value }]);
    }
    if indirect || !looks_number(operand) || operand.starts_with('@') || m == "STA" || m == "ISZ" {
        let op = m
            .parse::<MemoryOp>()
            .map_err(|_| err(line, AsmErrorKind::UnknownMnemonic(m.into())))?;
        return Ok(vec![Instruction::Memory {
            op,
            address: symbol_or_address(symbols, operand.trim_start_matches('@'), line)?,
            indirect,
        }]);
    }
    let op = m
        .parse::<ImmediateOp>()
        .map_err(|_| err(line, AsmErrorKind::UnknownMnemonic(m.into())))?;
    let value = if matches!(op, ImmediateOp::And | ImmediateOp::Or | ImmediateOp::Xor) {
        parse_raw16(operand, line)?
    } else {
        parse_signed16(operand, line)?
    };
    Ok(vec![Instruction::Immediate { op, value }])
}

fn symbol_or_address(
    map: &HashMap<&str, Address>,
    name: &str,
    line: usize,
) -> Result<Address, AsmError> {
    if looks_number(name) {
        parse_address_number(name)
            .map(|v| Address::new(v).unwrap())
            .map_err(|k| err(line, k))
    } else {
        map.get(name)
            .copied()
            .ok_or_else(|| err(line, AsmErrorKind::UndefinedSymbol(name.into())))
    }
}
fn valid_identifier(s: &str) -> bool {
    let mut c = s.chars();
    c.next()
        .is_some_and(|x| x == '_' || x.is_ascii_alphabetic())
        && c.all(|x| x == '_' || x.is_ascii_alphanumeric())
}
fn looks_number(s: &str) -> bool {
    let s = s.strip_prefix('@').unwrap_or(s);
    s.starts_with("0x")
        || s.starts_with("0X")
        || s.starts_with('+')
        || s.starts_with('-')
        || s.chars().next().is_some_and(|c| c.is_ascii_digit())
}
fn parse_i64(s: &str) -> Result<i64, AsmErrorKind> {
    if let Some(x) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        i64::from_str_radix(x, 16).map_err(|_| AsmErrorKind::InvalidNumber(s.into()))
    } else {
        s.parse::<i64>()
            .map_err(|_| AsmErrorKind::InvalidNumber(s.into()))
    }
}
fn parse_address_number(s: &str) -> Result<u16, AsmErrorKind> {
    let v = parse_i64(s)?;
    if (0..=0xffff).contains(&v) {
        Ok(v as u16)
    } else {
        Err(AsmErrorKind::AddressOutOfRange(v.max(0) as u32))
    }
}
fn parse_org(s: &str) -> Result<u16, AsmErrorKind> {
    let digits = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    let value =
        u32::from_str_radix(digits, 16).map_err(|_| AsmErrorKind::InvalidNumber(s.into()))?;
    if value <= 0xffff {
        Ok(value as u16)
    } else {
        Err(AsmErrorKind::AddressOutOfRange(value))
    }
}
fn parse_signed16(s: &str, line: usize) -> Result<Immediate16, AsmError> {
    let v = parse_i64(s).map_err(|k| err(line, k))?;
    let hexadecimal = s.starts_with("0x") || s.starts_with("0X");
    if (-32768..=32767).contains(&v) || (hexadecimal && v <= 0xffff) {
        Ok(Immediate16::from_raw(v as u16))
    } else {
        Err(err(line, AsmErrorKind::ImmediateOutOfRange(v)))
    }
}
fn parse_raw16(s: &str, line: usize) -> Result<Immediate16, AsmError> {
    let v = parse_i64(s).map_err(|k| err(line, k))?;
    if (-32768..=65535).contains(&v) {
        Ok(Immediate16::from_raw(v as u16))
    } else {
        Err(err(line, AsmErrorKind::ImmediateOutOfRange(v)))
    }
}
fn parse_hex_word(s: &str) -> Result<u32, AsmErrorKind> {
    let x = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u32::from_str_radix(x, 16).map_err(|_| AsmErrorKind::InvalidNumber(s.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assembles_all_formats_and_pseudos() {
        let s="ORG 0x0000\nJMP START\nORG 0x0010\nSTART: LDA -1\nADD VALUE\nCMP 1\nBEQ DONE\nPUSH\nPOP\nDONE: HLT\nVALUE: HEX 00000001\nEND\n";
        let r = Assembler::new().assemble(s).unwrap();
        assert_eq!(r.symbols["START"].get(), 0x10);
        assert_eq!(r.image.cells.len(), 11);
        assert_eq!(r.image.cells[0].word, 0x6000_0010);
        assert_eq!(r.image.cells[1].word, 0x2500_ffff);
    }
    #[test]
    fn indirect_and_sp_relative() {
        let r = Assembler::new()
            .assemble("ORG 0x10\nP: SYM V\nV: HEX 2a\nLDA P I\nLDSP -2\nADJSP 3\nEND")
            .unwrap();
        assert_eq!(r.image.cells[2].word, 0x0501_0010);
        assert_eq!(r.image.cells[3].word, 0x4500_fffe);
    }
    #[test]
    fn org_uses_hexadecimal_address_syntax() {
        let r = Assembler::new().assemble("ORG 0010\nHLT\nEND").unwrap();
        assert_eq!(r.image.cells[0].address.get(), 0x0010);
    }
    #[test]
    fn detects_overlap_and_duplicate() {
        let e = Assembler::new()
            .assemble("ORG 10\nA: HLT\nORG 10\nA: HLT\nEND")
            .unwrap_err();
        assert!(e
            .0
            .iter()
            .any(|x| matches!(x.kind, AsmErrorKind::DuplicateLabel(_))));
    }
}
