//! EX3ソースをメモリイメージへ変換する2パスアセンブラ。
//!
//! パーサはラベルを未解決の文字列として保持する。第1パスで全ラベルの
//! アドレスを収集し、第2パスで命令を解決するため、前方参照も利用できる。

use crate::{
    isa::{
        Address, Immediate12, ImmediateOp, Instruction, MemoryImmediateOp, N1Op, N2Op, NoOperandOp,
        Word,
    },
    CompatibilityMode,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    error::Error,
    fmt,
};

/// アセンブリ診断のソース位置（1始まり）。
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
        for (i, error) in self.0.iter().enumerate() {
            if i != 0 {
                writeln!(f)?;
            }
            write!(f, "{error}")?;
        }
        Ok(())
    }
}
impl Error for AsmErrors {}

/// 出力セルの由来。`.prb`生成時にデータセルだけを選ぶために使う。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellKind {
    Instruction,
    Data,
    Symbol,
}
/// アセンブル結果の1ワード。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryCell {
    pub address: Address,
    pub word: Word,
    pub kind: CellKind,
}
/// アドレス順ではなく、ソース上で生成された順序を保持するメモリイメージ。
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
#[derive(Clone, Debug)]
struct Line {
    line: usize,
    org: Option<String>,
    labels: Vec<String>,
    statement: Option<Unresolved>,
}

#[derive(Clone, Copy, Debug)]
pub struct Assembler {
    mode: CompatibilityMode,
}
impl Default for Assembler {
    fn default() -> Self {
        Self::new()
    }
}
impl Assembler {
    pub const fn new() -> Self {
        Self {
            mode: CompatibilityMode::Strict,
        }
    }
    pub const fn compatibility(mut self, mode: CompatibilityMode) -> Self {
        self.mode = mode;
        self
    }

    /// ソース全体を解析し、シンボル表とメモリイメージを生成する。
    pub fn assemble(&self, source: &str) -> Result<AssemblyResult, AsmErrors> {
        let lines = parse(source)?;
        let mut errors = Vec::new();
        let mut symbols = BTreeMap::new();
        let mut lc: u32 = 0;

        // Pass 1: ORGを反映しながらlocation counterを進め、ラベルを収集する。
        // この段階では命令オペランドを解決しないため、前方参照が可能になる。
        for line in &lines {
            if let Some(org) = &line.org {
                match parse_hex(org).filter(|v| *v <= 0xfff) {
                    Some(v) => lc = v,
                    None => errors.push(err(
                        line.line,
                        AsmErrorKind::AddressOutOfRange(parse_hex(org).unwrap_or(u32::MAX)),
                    )),
                }
            }
            for label in &line.labels {
                if !valid_identifier(label) || label == "I" {
                    errors.push(err(
                        line.line,
                        AsmErrorKind::Syntax(format!("invalid label `{label}`")),
                    ));
                } else if symbols.contains_key(label) {
                    errors.push(err(line.line, AsmErrorKind::DuplicateLabel(label.clone())));
                } else if lc > 0xfff {
                    errors.push(err(line.line, AsmErrorKind::AddressOutOfRange(lc)));
                } else {
                    symbols.insert(label.clone(), Address::new(lc as u16).expect("checked"));
                }
            }
            if line.statement.is_some() {
                lc += 1;
            }
        }
        if !errors.is_empty() {
            return Err(AsmErrors(errors));
        }

        // Pass 2で何度も参照するため、所有済みシンボル表から借用lookupを作る。
        let lookup: HashMap<_, _> = symbols.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        let mut image = MemoryImage::default();
        let mut used = HashSet::new();
        lc = 0;

        // Pass 2: ラベル参照をAddressへ変換し、命令をencodeする。
        for line in &lines {
            if let Some(org) = &line.org {
                lc = parse_hex(org).expect("pass 1 validated");
            }
            let Some(statement) = &line.statement else {
                continue;
            };
            if lc > 0xfff {
                errors.push(err(line.line, AsmErrorKind::AddressOutOfRange(lc)));
                lc += 1;
                continue;
            }
            let address = Address::new(lc as u16).expect("checked");
            if self.mode == CompatibilityMode::Strict && !used.insert(address) {
                errors.push(err(line.line, AsmErrorKind::OverlappingAddress(address)));
                lc += 1;
                continue;
            }
            match resolve(statement, &lookup, line.line) {
                Ok((word, kind)) => image.cells.push(MemoryCell {
                    address,
                    word,
                    kind,
                }),
                Err(e) => errors.push(e),
            }
            lc += 1;
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
        // `/`は旧Scala構文、`;`はRust版の拡張コメント構文。
        let text = raw.split(['/', ';']).next().unwrap_or(raw).trim();
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
        let mut rest = text;
        let mut labels = Vec::new();
        while let Some((before, after)) = rest.split_once(',') {
            let candidate = before.trim();
            if candidate.split_whitespace().count() != 1 {
                break;
            }
            labels.push(candidate.to_string());
            rest = after.trim_start();
        }
        let tokens: Vec<String> = rest.split_whitespace().map(str::to_string).collect();
        let (org, statement) = if tokens.is_empty() {
            (None, None)
        } else if tokens[0].eq_ignore_ascii_case("ORG") {
            if tokens.len() != 2 {
                errors.push(err(
                    line_no,
                    AsmErrorKind::Syntax("ORG requires one hexadecimal address".into()),
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
    if !ended {
        errors.push(err(source.lines().count().max(1), AsmErrorKind::MissingEnd));
    }
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
) -> Result<(Word, CellKind), AsmError> {
    match stmt {
        Unresolved::Hex(s) => parse_hex(s)
            .ok_or_else(|| err(line, AsmErrorKind::InvalidNumber(s.clone())))
            .map(|v| (v, CellKind::Data)),
        Unresolved::Dec(s) => s
            .parse::<i32>()
            .map(|v| (v as u32, CellKind::Data))
            .map_err(|_| err(line, AsmErrorKind::InvalidNumber(s.clone()))),
        Unresolved::Chr(s) => {
            let inner = if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
                &s[1..s.len() - 1]
            } else {
                s.as_str()
            };
            let mut chars = inner.chars();
            let c = chars
                .next()
                .filter(|_| chars.next().is_none())
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                .ok_or_else(|| {
                    err(
                        line,
                        AsmErrorKind::Syntax("CHR requires one [_0-9a-zA-Z] character".into()),
                    )
                })?;
            Ok((c as u32, CellKind::Data))
        }
        Unresolved::Sym(s) => symbol(symbols, s, line).map(|a| (a.get() as u32, CellKind::Symbol)),
        Unresolved::Instruction { mnemonic, operands } => {
            resolve_instruction(mnemonic, operands, symbols, line)
                .map(|i| (i.encode(), CellKind::Instruction))
        }
    }
}

fn resolve_instruction(
    m: &str,
    o: &[String],
    symbols: &HashMap<&str, Address>,
    line: usize,
) -> Result<Instruction, AsmError> {
    // 旧文法では末尾の大文字Iだけが間接指定として予約されている。
    let indirect = o.last().is_some_and(|s| s == "I");
    let args = if indirect { &o[..o.len() - 1] } else { o };
    if o.last()
        .is_some_and(|s| s.eq_ignore_ascii_case("i") && s != "I")
    {
        return Err(err(
            line,
            AsmErrorKind::Syntax("indirect marker must be uppercase I".into()),
        ));
    }
    if args.is_empty() {
        if indirect {
            return Err(err(line, AsmErrorKind::Syntax("unexpected I".into())));
        }
        let op = no_operand(m).ok_or_else(|| err(line, AsmErrorKind::UnknownMnemonic(m.into())))?;
        return Ok(Instruction::NoOperand(op));
    }
    if args.len() == 1 {
        // `LDA 1`は即値、`LDA LABEL`はメモリ参照。数値アドレスは
        // 意図的に導入せず、旧パーサの曖昧性解消規則を維持する。
        if looks_number(&args[0]) {
            if indirect {
                return Err(err(
                    line,
                    AsmErrorKind::Syntax("immediate instruction cannot be indirect".into()),
                ));
            }
            let op = immediate_op(m).ok_or_else(|| {
                err(
                    line,
                    AsmErrorKind::Syntax(format!("{m} does not accept an immediate operand")),
                )
            })?;
            return Ok(Instruction::Immediate {
                op,
                value: parse_immediate(&args[0], line)?,
            });
        }
        let op = n1_op(m).ok_or_else(|| {
            err(
                line,
                AsmErrorKind::Syntax(format!("{m} does not accept one address operand")),
            )
        })?;
        return Ok(Instruction::N1 {
            op,
            operand: symbol(symbols, &args[0], line)?,
            indirect,
        });
    }
    if args.len() == 2 {
        if looks_number(&args[1]) {
            let op = memory_immediate_op(m).ok_or_else(|| {
                err(
                    line,
                    AsmErrorKind::Syntax(format!("{m} does not accept address + immediate")),
                )
            })?;
            return Ok(Instruction::MemoryImmediate {
                op,
                operand: symbol(symbols, &args[0], line)?,
                value: parse_immediate(&args[1], line)?,
                indirect,
            });
        }
        let op = n2_op(m).ok_or_else(|| {
            err(
                line,
                AsmErrorKind::Syntax(format!("{m} does not accept two address operands")),
            )
        })?;
        return Ok(Instruction::N2 {
            op,
            operand1: symbol(symbols, &args[0], line)?,
            operand2: symbol(symbols, &args[1], line)?,
            indirect,
        });
    }
    Err(err(
        line,
        AsmErrorKind::Syntax(format!("wrong operand count for {m}")),
    ))
}

fn symbol(map: &HashMap<&str, Address>, name: &str, line: usize) -> Result<Address, AsmError> {
    map.get(name)
        .copied()
        .ok_or_else(|| err(line, AsmErrorKind::UndefinedSymbol(name.into())))
}
fn valid_identifier(s: &str) -> bool {
    let mut c = s.chars();
    c.next()
        .is_some_and(|x| x == '_' || x.is_ascii_alphabetic())
        && c.all(|x| x == '_' || x.is_ascii_alphanumeric())
}
fn parse_hex(s: &str) -> Option<u32> {
    u32::from_str_radix(
        s.strip_prefix("0x")
            .or_else(|| s.strip_prefix("0X"))
            .unwrap_or(s),
        16,
    )
    .ok()
}
fn looks_number(s: &str) -> bool {
    s.starts_with("0x")
        || s.starts_with("0X")
        || s.starts_with('+')
        || s.starts_with('-')
        || s.chars().next().is_some_and(|c| c.is_ascii_digit())
}
fn parse_immediate(s: &str, line: usize) -> Result<Immediate12, AsmError> {
    if s.starts_with("0x") || s.starts_with("0X") {
        let v = parse_hex(s).ok_or_else(|| err(line, AsmErrorKind::InvalidNumber(s.into())))?;
        if v > 0xfff {
            return Err(err(line, AsmErrorKind::ImmediateOutOfRange(v as i64)));
        }
        Immediate12::from_raw(v as u16)
            .map_err(|_| err(line, AsmErrorKind::ImmediateOutOfRange(v as i64)))
    } else {
        let v = s
            .parse::<i64>()
            .map_err(|_| err(line, AsmErrorKind::InvalidNumber(s.into())))?;
        if !(-2048..=2047).contains(&v) {
            return Err(err(line, AsmErrorKind::ImmediateOutOfRange(v)));
        }
        Immediate12::from_signed(v as i32)
            .map_err(|_| err(line, AsmErrorKind::ImmediateOutOfRange(v)))
    }
}
fn n1_op(s: &str) -> Option<N1Op> {
    Some(match s {
        "ADD" => N1Op::Add,
        "SUB" => N1Op::Sub,
        "AND" => N1Op::And,
        "OR" => N1Op::Or,
        "XOR" => N1Op::Xor,
        "LDA" => N1Op::Lda,
        "STA" => N1Op::Sta,
        "BUN" => N1Op::Bun,
        "BSA" => N1Op::Bsa,
        "JPA" => N1Op::Jpa,
        "JZA" => N1Op::Jza,
        "JNA" => N1Op::Jna,
        "JZE" => N1Op::Jze,
        "ISZ" => N1Op::Isz,
        _ => return None,
    })
}
fn n2_op(s: &str) -> Option<N2Op> {
    Some(match s {
        "ADD" => N2Op::Add,
        "SUB" => N2Op::Sub,
        "AND" => N2Op::And,
        "OR" => N2Op::Or,
        "XOR" => N2Op::Xor,
        "MOVE" => N2Op::Move,
        _ => return None,
    })
}
fn immediate_op(s: &str) -> Option<ImmediateOp> {
    Some(match s {
        "ADD" => ImmediateOp::Add,
        "AND" => ImmediateOp::And,
        "OR" => ImmediateOp::Or,
        "LDA" => ImmediateOp::Lda,
        _ => return None,
    })
}
fn memory_immediate_op(s: &str) -> Option<MemoryImmediateOp> {
    Some(match s {
        "ADD" => MemoryImmediateOp::Add,
        "AND" => MemoryImmediateOp::And,
        "OR" => MemoryImmediateOp::Or,
        "STA" => MemoryImmediateOp::Sta,
        _ => return None,
    })
}
fn no_operand(s: &str) -> Option<NoOperandOp> {
    Some(match s {
        "CLA" => NoOperandOp::Cla,
        "CLE" => NoOperandOp::Cle,
        "CMA" => NoOperandOp::Cma,
        "CME" => NoOperandOp::Cme,
        "CIR" => NoOperandOp::Cir,
        "CIL" => NoOperandOp::Cil,
        "INC" => NoOperandOp::Inc,
        "SPA" => NoOperandOp::Spa,
        "SZA" => NoOperandOp::Sza,
        "SNA" => NoOperandOp::Sna,
        "SZE" => NoOperandOp::Sze,
        "INP" => NoOperandOp::Inp,
        "OUT" => NoOperandOp::Out,
        "SKI" => NoOperandOp::Ski,
        "SKO" => NoOperandOp::Sko,
        "ION" => NoOperandOp::Ion,
        "IOF" => NoOperandOp::Iof,
        "SIO" => NoOperandOp::Sio,
        "PIO" => NoOperandOp::Pio,
        "IMK" => NoOperandOp::Imk,
        "HLT" => NoOperandOp::Hlt,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_and_outputs() {
        let s="ORG 010\nSTART, LDA COUNT\n ADD 1\n STA COUNT\n JZA ZERO\n BUN START\nZERO, HLT\nCOUNT, DEC -1\nEND\n";
        let r = Assembler::new().assemble(s).unwrap();
        assert_eq!(r.symbols["START"].get(), 0x10);
        assert_eq!(r.image.cells[0].word, 0x00020016);
        assert_eq!(r.image.cells[1].word, 0xc1000001);
        assert_eq!(r.image.cells.last().unwrap().word, 0xffff_ffff);
    }
    #[test]
    fn forward_sym_and_multiple_labels() {
        let r = Assembler::new()
            .assemble("A, B, SYM C\nC, HEX deadbeef\nEND")
            .unwrap();
        assert_eq!(r.symbols["A"], r.symbols["B"]);
        assert_eq!(r.image.cells[0].word, 1);
    }
    #[test]
    fn aggregates_errors() {
        let e = Assembler::new()
            .assemble("A, LDA X\nA, BUN Y\nEND")
            .unwrap_err();
        assert!(e
            .0
            .iter()
            .any(|e| matches!(e.kind, AsmErrorKind::DuplicateLabel(_))));
    }
    #[test]
    fn missing_end() {
        assert!(matches!(
            Assembler::new().assemble("HLT").unwrap_err().0[0].kind,
            AsmErrorKind::MissingEnd
        ));
    }
    #[test]
    fn all_formats() {
        let s="ORG 010\nA, HEX 1\nB, HEX 2\nADD A\nADD A I\nADD A B\nMOVE A B I\nLDA -1\nSTA A 0xfff I\nCLA\nEND";
        let r = Assembler::new().assemble(s).unwrap();
        assert_eq!(r.image.cells.len(), 9);
    }
}
