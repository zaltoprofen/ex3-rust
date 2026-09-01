use super::{
    lexer::{Token, TokenKind},
    CcError, Span,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Int,
    UInt,
    Void,
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
    Number(u32, bool),
    Var(String),
    Call(String, Vec<Expr>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Assign(String, Box<Expr>),
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
    Expr(Expr),
    Block(Vec<Stmt>),
    Decl(String, Type, Option<Expr>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>),
    Switch(Expr, Vec<SwitchPart>),
    Break,
    Continue,
    Goto(String),
    Label(String, Box<Stmt>),
    Return(Option<Expr>),
}
#[derive(Clone, Debug)]
pub enum SwitchPart {
    Case(Expr, Span),
    Default(Span),
    Stmt(Stmt),
}

pub fn parse(tokens: Vec<Token>) -> Result<Program, Vec<CcError>> {
    Parser { tokens, pos: 0 }.program().map_err(|e| vec![e])
}
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}
impl Parser {
    fn program(&mut self) -> Result<Program, CcError> {
        let mut items = vec![];
        while !matches!(self.peek().kind, TokenKind::Eof) {
            items.push(self.item()?);
        }
        Ok(Program { items })
    }
    fn item(&mut self) -> Result<Item, CcError> {
        let span = self.peek().span;
        let ty = self.parse_type()?;
        let name = self.ident()?;
        if self.eat("(") {
            let params = self.params()?;
            self.expect(")")?;
            let body = if self.eat(";") {
                None
            } else {
                Some(self.stmt()?)
            };
            Ok(Item::Function(Function {
                name,
                ret: ty,
                params,
                body,
                span,
            }))
        } else {
            if ty == Type::Void {
                return Err(CcError::new(span, "variable cannot have type void"));
            }
            let init = if self.eat("=") {
                Some(self.expr()?)
            } else {
                None
            };
            self.expect(";")?;
            Ok(Item::Global(Global {
                name,
                ty,
                init,
                span,
            }))
        }
    }
    fn params(&mut self) -> Result<Vec<Param>, CcError> {
        if self.is_kw("void") {
            let save = self.pos;
            self.pos += 1;
            if self.is_sym(")") {
                return Ok(vec![]);
            }
            self.pos = save;
        }
        if self.is_sym(")") {
            return Ok(vec![]);
        }
        let mut ps = vec![];
        loop {
            let span = self.peek().span;
            let ty = self.parse_type()?;
            if ty == Type::Void {
                return Err(CcError::new(
                    span,
                    "void parameter is only valid as `(void)`",
                ));
            }
            ps.push(Param {
                name: self.ident()?,
                ty,
                span,
            });
            if !self.eat(",") {
                break;
            }
        }
        Ok(ps)
    }
    fn stmt(&mut self) -> Result<Stmt, CcError> {
        let span = self.peek().span;
        if self.eat("{") {
            let mut xs = vec![];
            while !self.eat("}") {
                if matches!(self.peek().kind, TokenKind::Eof) {
                    return Err(CcError::new(span, "unterminated block"));
                }
                xs.push(self.stmt()?)
            }
            return Ok(Stmt {
                kind: StmtKind::Block(xs),
                span,
            });
        }
        if self.eat(";") {
            return Ok(Stmt {
                kind: StmtKind::Empty,
                span,
            });
        }
        if self.eat_kw("if") {
            self.expect("(")?;
            let c = self.expr()?;
            self.expect(")")?;
            let a = Box::new(self.stmt()?);
            let b = if self.eat_kw("else") {
                Some(Box::new(self.stmt()?))
            } else {
                None
            };
            return Ok(Stmt {
                kind: StmtKind::If(c, a, b),
                span,
            });
        }
        if self.eat_kw("while") {
            self.expect("(")?;
            let c = self.expr()?;
            self.expect(")")?;
            let b = Box::new(self.stmt()?);
            return Ok(Stmt {
                kind: StmtKind::While(c, b),
                span,
            });
        }
        if self.eat_kw("switch") {
            self.expect("(")?;
            let e = self.expr()?;
            self.expect(")")?;
            self.expect("{")?;
            let mut p = vec![];
            while !self.eat("}") {
                if self.eat_kw("case") {
                    let s = self.prev_span();
                    let e = self.expr()?;
                    self.expect(":")?;
                    p.push(SwitchPart::Case(e, s))
                } else if self.eat_kw("default") {
                    let s = self.prev_span();
                    self.expect(":")?;
                    p.push(SwitchPart::Default(s))
                } else {
                    p.push(SwitchPart::Stmt(self.stmt()?))
                }
            }
            return Ok(Stmt {
                kind: StmtKind::Switch(e, p),
                span,
            });
        }
        if self.eat_kw("break") {
            self.expect(";")?;
            return Ok(Stmt {
                kind: StmtKind::Break,
                span,
            });
        }
        if self.eat_kw("continue") {
            self.expect(";")?;
            return Ok(Stmt {
                kind: StmtKind::Continue,
                span,
            });
        }
        if self.eat_kw("goto") {
            let n = self.ident()?;
            self.expect(";")?;
            return Ok(Stmt {
                kind: StmtKind::Goto(n),
                span,
            });
        }
        if self.eat_kw("return") {
            let e = if self.eat(";") {
                None
            } else {
                let x = self.expr()?;
                self.expect(";")?;
                Some(x)
            };
            return Ok(Stmt {
                kind: StmtKind::Return(e),
                span,
            });
        }
        if self.starts_type() {
            let ty = self.parse_type()?;
            if ty == Type::Void {
                return Err(CcError::new(span, "local variable cannot have type void"));
            }
            let n = self.ident()?;
            let e = if self.eat("=") {
                Some(self.expr()?)
            } else {
                None
            };
            self.expect(";")?;
            return Ok(Stmt {
                kind: StmtKind::Decl(n, ty, e),
                span,
            });
        }
        if let TokenKind::Ident(n) = &self.peek().kind {
            if self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|t| matches!(t.kind, TokenKind::Symbol(":")))
            {
                let n = n.clone();
                self.pos += 2;
                let s = Box::new(self.stmt()?);
                return Ok(Stmt {
                    kind: StmtKind::Label(n, s),
                    span,
                });
            }
        }
        let e = self.expr()?;
        self.expect(";")?;
        Ok(Stmt {
            kind: StmtKind::Expr(e),
            span,
        })
    }
    fn expr(&mut self) -> Result<Expr, CcError> {
        self.assign()
    }
    fn assign(&mut self) -> Result<Expr, CcError> {
        let lhs = self.binary(1)?;
        if self.eat("=") {
            let span = lhs.span;
            let ExprKind::Var(n) = lhs.kind else {
                return Err(CcError::new(
                    span,
                    "left side of assignment must be a variable",
                ));
            };
            let rhs = self.assign()?;
            Ok(Expr {
                kind: ExprKind::Assign(n, Box::new(rhs)),
                span,
            })
        } else {
            Ok(lhs)
        }
    }
    fn binary(&mut self, min: u8) -> Result<Expr, CcError> {
        let mut lhs = self.unary()?;
        while let Some((op, p)) = self.binop() {
            if p < min {
                break;
            }
            self.pos += 1;
            let rhs = self.binary(p + 1)?;
            let span = lhs.span;
            lhs = Expr {
                kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)),
                span,
            }
        }
        Ok(lhs)
    }
    fn binop(&self) -> Option<(BinOp, u8)> {
        let TokenKind::Symbol(s) = self.peek().kind else {
            return None;
        };
        Some(match s {
            "||" => (BinOp::Or, 1),
            "&&" => (BinOp::And, 2),
            "|" => (BinOp::BitOr, 3),
            "^" => (BinOp::BitXor, 4),
            "&" => (BinOp::BitAnd, 5),
            "==" => (BinOp::Eq, 6),
            "!=" => (BinOp::Ne, 6),
            "<" => (BinOp::Lt, 7),
            "<=" => (BinOp::Le, 7),
            ">" => (BinOp::Gt, 7),
            ">=" => (BinOp::Ge, 7),
            "+" => (BinOp::Add, 8),
            "-" => (BinOp::Sub, 8),
            "*" => (BinOp::Mul, 9),
            "/" => (BinOp::Div, 9),
            "%" => (BinOp::Mod, 9),
            _ => return None,
        })
    }
    fn unary(&mut self) -> Result<Expr, CcError> {
        let span = self.peek().span;
        let op = if self.eat("~") {
            Some(UnOp::BitNot)
        } else if self.eat("!") {
            Some(UnOp::Not)
        } else if self.eat("+") {
            Some(UnOp::Plus)
        } else if self.eat("-") {
            Some(UnOp::Neg)
        } else {
            None
        };
        if let Some(op) = op {
            let e = self.unary()?;
            Ok(Expr {
                kind: ExprKind::Unary(op, Box::new(e)),
                span,
            })
        } else {
            self.primary()
        }
    }
    fn primary(&mut self) -> Result<Expr, CcError> {
        let t = self.peek().clone();
        match t.kind {
            TokenKind::Number(v, u) => {
                self.pos += 1;
                Ok(Expr {
                    kind: ExprKind::Number(v, u),
                    span: t.span,
                })
            }
            TokenKind::Ident(n) => {
                self.pos += 1;
                if self.eat("(") {
                    let mut a = vec![];
                    if !self.eat(")") {
                        loop {
                            a.push(self.expr()?);
                            if self.eat(")") {
                                break;
                            }
                            self.expect(",")?
                        }
                    }
                    Ok(Expr {
                        kind: ExprKind::Call(n, a),
                        span: t.span,
                    })
                } else {
                    Ok(Expr {
                        kind: ExprKind::Var(n),
                        span: t.span,
                    })
                }
            }
            TokenKind::Symbol("(") => {
                self.pos += 1;
                let e = self.expr()?;
                self.expect(")")?;
                Ok(e)
            }
            _ => Err(CcError::new(t.span, "expected expression")),
        }
    }
    fn parse_type(&mut self) -> Result<Type, CcError> {
        let s = self.peek().span;
        if self.eat_kw("int") || self.eat_kw("int32_t") {
            Ok(Type::Int)
        } else if self.eat_kw("uint32_t") {
            Ok(Type::UInt)
        } else if self.eat_kw("unsigned") {
            if self.eat_kw("int") {
                Ok(Type::UInt)
            } else {
                Err(CcError::new(s, "`unsigned` must be followed by `int`"))
            }
        } else if self.eat_kw("void") {
            Ok(Type::Void)
        } else {
            Err(CcError::new(s, "expected type"))
        }
    }
    fn starts_type(&self) -> bool {
        matches!(&self.peek().kind,TokenKind::Ident(x) if matches!(x.as_str(),"int"|"int32_t"|"uint32_t"|"unsigned"|"void"))
    }
    fn ident(&mut self) -> Result<String, CcError> {
        let t = self.peek().clone();
        if let TokenKind::Ident(n) = t.kind {
            if Self::keyword(&n) {
                Err(CcError::new(t.span, "expected identifier"))
            } else {
                self.pos += 1;
                Ok(n)
            }
        } else {
            Err(CcError::new(t.span, "expected identifier"))
        }
    }
    fn keyword(s: &str) -> bool {
        matches!(
            s,
            "int"
                | "int32_t"
                | "uint32_t"
                | "unsigned"
                | "void"
                | "if"
                | "else"
                | "while"
                | "switch"
                | "case"
                | "default"
                | "break"
                | "continue"
                | "goto"
                | "return"
        )
    }
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }
    fn prev_span(&self) -> Span {
        self.tokens[self.pos - 1].span
    }
    fn is_kw(&self, s: &str) -> bool {
        matches!(&self.peek().kind,TokenKind::Ident(x) if x==s)
    }
    fn eat_kw(&mut self, s: &str) -> bool {
        if self.is_kw(s) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn is_sym(&self, s: &str) -> bool {
        matches!(self.peek().kind,TokenKind::Symbol(x) if x==s)
    }
    fn eat(&mut self, s: &str) -> bool {
        if self.is_sym(s) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, s: &str) -> Result<(), CcError> {
        if self.eat(s) {
            Ok(())
        } else {
            Err(CcError::new(self.peek().span, format!("expected `{s}`")))
        }
    }
}
