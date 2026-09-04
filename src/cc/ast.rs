use super::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Scalar(ScalarType),
    Void,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarType {
    Int32,
    UInt32,
}

impl Type {
    pub const INT: Self = Self::Scalar(ScalarType::Int32);
    pub const UINT: Self = Self::Scalar(ScalarType::UInt32);

    pub fn scalar(self) -> Option<ScalarType> {
        match self {
            Self::Scalar(ty) => Some(ty),
            Self::Void => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntegerLiteral {
    pub value: u32,
    pub kind: IntegerLiteralKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerLiteralKind {
    Signed,
    Unsigned,
}

impl IntegerLiteral {
    pub fn ty(self) -> ScalarType {
        match self.kind {
            IntegerLiteralKind::Signed => ScalarType::Int32,
            IntegerLiteralKind::Unsigned => ScalarType::UInt32,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Clone, Debug)]
pub enum Item {
    Global(Global),
    Function(Function),
}

#[derive(Clone, Debug)]
pub struct Global {
    pub name: String,
    pub ty: Type,
    pub init: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    pub ret: Type,
    pub params: Vec<Param>,
    pub body: Option<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Integer(IntegerLiteral),
    Variable {
        name: String,
    },
    Call {
        function: String,
        arguments: Vec<Expr>,
    },
    Unary {
        op: UnOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Assign {
        target: String,
        value: Box<Expr>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    BitNot,
    Not,
    Plus,
    Neg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Mul,
    Div,
    Mod,
    Add,
    Sub,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    BitAnd,
    BitXor,
    BitOr,
    And,
    Or,
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Empty,
    Expression {
        expression: Expr,
    },
    Block {
        statements: Vec<Stmt>,
    },
    Declaration {
        name: String,
        ty: Type,
        initializer: Option<Expr>,
    },
    If {
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    While {
        condition: Expr,
        body: Box<Stmt>,
    },
    Switch {
        expression: Expr,
        parts: Vec<SwitchPart>,
    },
    Break,
    Continue,
    Goto {
        label: String,
    },
    Label {
        name: String,
        body: Box<Stmt>,
    },
    Return {
        value: Option<Expr>,
    },
}

#[derive(Clone, Debug)]
pub enum SwitchPart {
    Case { value: Expr, span: Span },
    Default { span: Span },
    Statement { statement: Stmt },
}
