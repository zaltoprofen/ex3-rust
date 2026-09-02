use crate::cc::ast::{BinOp, ScalarType, Type, UnOp};

pub(crate) struct AnalyzedProgram {
    pub globals: Vec<ResolvedGlobal>,
    pub functions: Vec<ResolvedFunction>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LocalSlot(pub usize);

#[derive(Clone, Copy, Debug)]
pub(crate) struct ParamIndex(pub usize);

#[derive(Clone, Debug)]
pub(crate) struct GlobalSymbol(pub String);

#[derive(Clone, Copy, Debug)]
pub(crate) struct UserLabel(pub usize);

#[derive(Clone, Copy, Debug)]
pub(crate) enum BuiltinId {
    Putchar,
    Getchar,
}

#[derive(Clone, Debug)]
pub(crate) enum ResolvedCallee {
    User(String),
    Builtin(BuiltinId),
}

#[derive(Clone, Debug)]
pub(crate) enum ResolvedVariable {
    Local(LocalSlot),
    Parameter(ParamIndex),
    Global(GlobalSymbol),
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedExpr {
    pub kind: ResolvedExprKind,
    pub ty: Type,
}

#[derive(Clone, Debug)]
pub(crate) enum ResolvedExprKind {
    Number(u32),
    Load(ResolvedVariable),
    Assign {
        target: ResolvedVariable,
        value: Box<ResolvedExpr>,
    },
    Call {
        callee: ResolvedCallee,
        args: Vec<ResolvedExpr>,
    },
    Unary {
        op: UnOp,
        operand: Box<ResolvedExpr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<ResolvedExpr>,
        rhs: Box<ResolvedExpr>,
        operand_type: ScalarType,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum ResolvedStmt {
    Empty,
    Expr(ResolvedExpr),
    Block(Vec<ResolvedStmt>),
    Decl {
        slot: LocalSlot,
        init: Option<ResolvedExpr>,
    },
    If {
        condition: ResolvedExpr,
        then_stmt: Box<ResolvedStmt>,
        else_stmt: Option<Box<ResolvedStmt>>,
    },
    While {
        condition: ResolvedExpr,
        body: Box<ResolvedStmt>,
    },
    Switch {
        expression: ResolvedExpr,
        parts: Vec<ResolvedSwitchPart>,
    },
    Break,
    Continue,
    Goto(UserLabel),
    Label(UserLabel, Box<ResolvedStmt>),
    Return(Option<ResolvedExpr>),
}

#[derive(Clone, Debug)]
pub(crate) enum ResolvedSwitchPart {
    Case(u32),
    Default,
    Stmt(ResolvedStmt),
}

pub(crate) struct ResolvedGlobal {
    pub name: String,
    pub value: u32,
}

pub(crate) struct ResolvedFunction {
    pub name: String,
    pub body: ResolvedStmt,
    pub local_count: usize,
    pub parameter_count: usize,
}
