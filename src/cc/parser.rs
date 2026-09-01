use super::{
    ast::*,
    lexer::{Keyword, Symbol, Token, TokenKind},
    CcError, Span,
};

pub fn parse(tokens: Vec<Token>) -> Result<Program, Vec<CcError>> {
    Parser {
        tokens,
        position: 0,
    }
    .program()
    .map_err(|error| vec![error])
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Precedence {
    LogicalOr,
    LogicalAnd,
    BitwiseOr,
    BitwiseXor,
    BitwiseAnd,
    Equality,
    Relational,
    Additive,
    Multiplicative,
    Primary,
}

impl Precedence {
    fn next(self) -> Self {
        match self {
            Self::LogicalOr => Self::LogicalAnd,
            Self::LogicalAnd => Self::BitwiseOr,
            Self::BitwiseOr => Self::BitwiseXor,
            Self::BitwiseXor => Self::BitwiseAnd,
            Self::BitwiseAnd => Self::Equality,
            Self::Equality => Self::Relational,
            Self::Relational => Self::Additive,
            Self::Additive => Self::Multiplicative,
            Self::Multiplicative | Self::Primary => Self::Primary,
        }
    }
}

impl BinOp {
    fn from_symbol(symbol: Symbol) -> Option<Self> {
        Some(match symbol {
            Symbol::LogicalOr => Self::Or,
            Symbol::LogicalAnd => Self::And,
            Symbol::Pipe => Self::BitOr,
            Symbol::Caret => Self::BitXor,
            Symbol::Ampersand => Self::BitAnd,
            Symbol::EqualEqual => Self::Eq,
            Symbol::BangEqual => Self::Ne,
            Symbol::Less => Self::Lt,
            Symbol::LessEqual => Self::Le,
            Symbol::Greater => Self::Gt,
            Symbol::GreaterEqual => Self::Ge,
            Symbol::Plus => Self::Add,
            Symbol::Minus => Self::Sub,
            Symbol::Star => Self::Mul,
            Symbol::Slash => Self::Div,
            Symbol::Percent => Self::Mod,
            _ => return None,
        })
    }

    fn precedence(self) -> Precedence {
        match self {
            Self::Or => Precedence::LogicalOr,
            Self::And => Precedence::LogicalAnd,
            Self::BitOr => Precedence::BitwiseOr,
            Self::BitXor => Precedence::BitwiseXor,
            Self::BitAnd => Precedence::BitwiseAnd,
            Self::Eq | Self::Ne => Precedence::Equality,
            Self::Lt | Self::Le | Self::Gt | Self::Ge => Precedence::Relational,
            Self::Add | Self::Sub => Precedence::Additive,
            Self::Mul | Self::Div | Self::Mod => Precedence::Multiplicative,
        }
    }
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn program(&mut self) -> Result<Program, CcError> {
        let mut items = Vec::new();
        while !matches!(self.peek().kind, TokenKind::Eof) {
            items.push(self.item()?);
        }
        Ok(Program { items })
    }

    fn item(&mut self) -> Result<Item, CcError> {
        let span = self.peek().span;
        let ty = self.parse_type()?;
        let name = self.identifier()?;
        if self.eat_symbol(Symbol::LeftParen) {
            let params = self.parameters()?;
            self.expect_symbol(Symbol::RightParen)?;
            let body = if self.eat_symbol(Symbol::Semicolon) {
                None
            } else {
                Some(self.statement()?)
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
            let init = self
                .eat_symbol(Symbol::Assign)
                .then(|| self.expression())
                .transpose()?;
            self.expect_symbol(Symbol::Semicolon)?;
            Ok(Item::Global(Global {
                name,
                ty,
                init,
                span,
            }))
        }
    }

    fn parameters(&mut self) -> Result<Vec<Param>, CcError> {
        if self.is_keyword(Keyword::Void) && self.next_is_symbol(Symbol::RightParen) {
            self.position += 1;
            return Ok(Vec::new());
        }
        if self.is_symbol(Symbol::RightParen) {
            return Ok(Vec::new());
        }
        let mut parameters = Vec::new();
        loop {
            let span = self.peek().span;
            let ty = self.parse_type()?;
            if ty == Type::Void {
                return Err(CcError::new(
                    span,
                    "void parameter is only valid as `(void)`",
                ));
            }
            parameters.push(Param {
                name: self.identifier()?,
                ty,
                span,
            });
            if !self.eat_symbol(Symbol::Comma) {
                break;
            }
        }
        Ok(parameters)
    }

    fn statement(&mut self) -> Result<Stmt, CcError> {
        let span = self.peek().span;
        let kind = if self.eat_symbol(Symbol::LeftBrace) {
            let mut statements = Vec::new();
            while !self.eat_symbol(Symbol::RightBrace) {
                if matches!(self.peek().kind, TokenKind::Eof) {
                    return Err(CcError::new(span, "unterminated block"));
                }
                statements.push(self.statement()?);
            }
            StmtKind::Block { statements }
        } else if self.eat_symbol(Symbol::Semicolon) {
            StmtKind::Empty
        } else if self.eat_keyword(Keyword::If) {
            self.expect_symbol(Symbol::LeftParen)?;
            let condition = self.expression()?;
            self.expect_symbol(Symbol::RightParen)?;
            let then_branch = Box::new(self.statement()?);
            let else_branch = self
                .eat_keyword(Keyword::Else)
                .then(|| self.statement())
                .transpose()?
                .map(Box::new);
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            }
        } else if self.eat_keyword(Keyword::While) {
            self.expect_symbol(Symbol::LeftParen)?;
            let condition = self.expression()?;
            self.expect_symbol(Symbol::RightParen)?;
            StmtKind::While {
                condition,
                body: Box::new(self.statement()?),
            }
        } else if self.eat_keyword(Keyword::Switch) {
            self.switch_statement()?
        } else if self.eat_keyword(Keyword::Break) {
            self.expect_symbol(Symbol::Semicolon)?;
            StmtKind::Break
        } else if self.eat_keyword(Keyword::Continue) {
            self.expect_symbol(Symbol::Semicolon)?;
            StmtKind::Continue
        } else if self.eat_keyword(Keyword::Goto) {
            let label = self.identifier()?;
            self.expect_symbol(Symbol::Semicolon)?;
            StmtKind::Goto { label }
        } else if self.eat_keyword(Keyword::Return) {
            let value = if self.eat_symbol(Symbol::Semicolon) {
                None
            } else {
                let value = self.expression()?;
                self.expect_symbol(Symbol::Semicolon)?;
                Some(value)
            };
            StmtKind::Return { value }
        } else if self.starts_type() {
            let ty = self.parse_type()?;
            if ty == Type::Void {
                return Err(CcError::new(span, "local variable cannot have type void"));
            }
            let name = self.identifier()?;
            let initializer = self
                .eat_symbol(Symbol::Assign)
                .then(|| self.expression())
                .transpose()?;
            self.expect_symbol(Symbol::Semicolon)?;
            StmtKind::Declaration {
                name,
                ty,
                initializer,
            }
        } else if self.is_label() {
            let name = self.identifier()?;
            self.expect_symbol(Symbol::Colon)?;
            StmtKind::Label {
                name,
                body: Box::new(self.statement()?),
            }
        } else {
            let expression = self.expression()?;
            self.expect_symbol(Symbol::Semicolon)?;
            StmtKind::Expression { expression }
        };
        Ok(Stmt { kind, span })
    }

    fn switch_statement(&mut self) -> Result<StmtKind, CcError> {
        self.expect_symbol(Symbol::LeftParen)?;
        let expression = self.expression()?;
        self.expect_symbol(Symbol::RightParen)?;
        self.expect_symbol(Symbol::LeftBrace)?;
        let mut parts = Vec::new();
        while !self.eat_symbol(Symbol::RightBrace) {
            if self.eat_keyword(Keyword::Case) {
                let span = self.previous_span();
                let value = self.expression()?;
                self.expect_symbol(Symbol::Colon)?;
                parts.push(SwitchPart::Case { value, span });
            } else if self.eat_keyword(Keyword::Default) {
                let span = self.previous_span();
                self.expect_symbol(Symbol::Colon)?;
                parts.push(SwitchPart::Default { span });
            } else {
                parts.push(SwitchPart::Statement {
                    statement: self.statement()?,
                });
            }
        }
        Ok(StmtKind::Switch { expression, parts })
    }

    fn expression(&mut self) -> Result<Expr, CcError> {
        self.assignment()
    }

    fn assignment(&mut self) -> Result<Expr, CcError> {
        let lhs = self.binary(Precedence::LogicalOr)?;
        if !self.eat_symbol(Symbol::Assign) {
            return Ok(lhs);
        }
        let span = lhs.span;
        let ExprKind::Variable { name } = lhs.kind else {
            return Err(CcError::new(
                span,
                "left side of assignment must be a variable",
            ));
        };
        Ok(Expr {
            kind: ExprKind::Assign {
                target: name,
                value: Box::new(self.assignment()?),
            },
            span,
        })
    }

    fn binary(&mut self, minimum: Precedence) -> Result<Expr, CcError> {
        let mut lhs = self.unary()?;
        while let Some(operator) = self.binary_operator() {
            let precedence = operator.precedence();
            if precedence < minimum {
                break;
            }
            self.position += 1;
            let rhs = self.binary(precedence.next())?;
            let span = lhs.span;
            lhs = Expr {
                kind: ExprKind::Binary {
                    op: operator,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Ok(lhs)
    }

    fn binary_operator(&self) -> Option<BinOp> {
        let TokenKind::Symbol(symbol) = self.peek().kind else {
            return None;
        };
        BinOp::from_symbol(symbol)
    }

    fn unary(&mut self) -> Result<Expr, CcError> {
        let span = self.peek().span;
        let operator = if self.eat_symbol(Symbol::Tilde) {
            Some(UnOp::BitNot)
        } else if self.eat_symbol(Symbol::Bang) {
            Some(UnOp::Not)
        } else if self.eat_symbol(Symbol::Plus) {
            Some(UnOp::Plus)
        } else if self.eat_symbol(Symbol::Minus) {
            Some(UnOp::Neg)
        } else {
            None
        };
        if let Some(op) = operator {
            Ok(Expr {
                kind: ExprKind::Unary {
                    op,
                    operand: Box::new(self.unary()?),
                },
                span,
            })
        } else {
            self.primary()
        }
    }

    fn primary(&mut self) -> Result<Expr, CcError> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Integer(literal) => {
                self.position += 1;
                Ok(Expr {
                    kind: ExprKind::Integer(literal),
                    span: token.span,
                })
            }
            TokenKind::Identifier(name) => {
                self.position += 1;
                if self.eat_symbol(Symbol::LeftParen) {
                    let mut arguments = Vec::new();
                    if !self.eat_symbol(Symbol::RightParen) {
                        loop {
                            arguments.push(self.expression()?);
                            if self.eat_symbol(Symbol::RightParen) {
                                break;
                            }
                            self.expect_symbol(Symbol::Comma)?;
                        }
                    }
                    Ok(Expr {
                        kind: ExprKind::Call {
                            function: name,
                            arguments,
                        },
                        span: token.span,
                    })
                } else {
                    Ok(Expr {
                        kind: ExprKind::Variable { name },
                        span: token.span,
                    })
                }
            }
            TokenKind::Symbol(Symbol::LeftParen) => {
                self.position += 1;
                let expression = self.expression()?;
                self.expect_symbol(Symbol::RightParen)?;
                Ok(expression)
            }
            _ => Err(CcError::new(token.span, "expected expression")),
        }
    }

    fn parse_type(&mut self) -> Result<Type, CcError> {
        let span = self.peek().span;
        if self.eat_keyword(Keyword::Int) || self.eat_keyword(Keyword::Int32) {
            Ok(Type::INT)
        } else if self.eat_keyword(Keyword::UInt32) {
            Ok(Type::UINT)
        } else if self.eat_keyword(Keyword::Unsigned) {
            if self.eat_keyword(Keyword::Int) {
                Ok(Type::UINT)
            } else {
                Err(CcError::new(span, "`unsigned` must be followed by `int`"))
            }
        } else if self.eat_keyword(Keyword::Void) {
            Ok(Type::Void)
        } else {
            Err(CcError::new(span, "expected type"))
        }
    }

    fn starts_type(&self) -> bool {
        matches!(
            self.peek().kind,
            TokenKind::Keyword(
                Keyword::Int | Keyword::Int32 | Keyword::UInt32 | Keyword::Unsigned | Keyword::Void
            )
        )
    }
    fn is_label(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Identifier(_)) && self.next_is_symbol(Symbol::Colon)
    }
    fn identifier(&mut self) -> Result<String, CcError> {
        let token = self.peek().clone();
        if let TokenKind::Identifier(name) = token.kind {
            self.position += 1;
            Ok(name)
        } else {
            Err(CcError::new(token.span, "expected identifier"))
        }
    }
    fn peek(&self) -> &Token {
        &self.tokens[self.position]
    }
    fn previous_span(&self) -> Span {
        self.tokens[self.position - 1].span
    }
    fn is_keyword(&self, keyword: Keyword) -> bool {
        self.peek().kind == TokenKind::Keyword(keyword)
    }
    fn eat_keyword(&mut self, keyword: Keyword) -> bool {
        if self.is_keyword(keyword) {
            self.position += 1;
            true
        } else {
            false
        }
    }
    fn is_symbol(&self, symbol: Symbol) -> bool {
        self.peek().kind == TokenKind::Symbol(symbol)
    }
    fn next_is_symbol(&self, symbol: Symbol) -> bool {
        self.tokens
            .get(self.position + 1)
            .is_some_and(|token| token.kind == TokenKind::Symbol(symbol))
    }
    fn eat_symbol(&mut self, symbol: Symbol) -> bool {
        if self.is_symbol(symbol) {
            self.position += 1;
            true
        } else {
            false
        }
    }
    fn expect_symbol(&mut self, symbol: Symbol) -> Result<(), CcError> {
        if self.eat_symbol(symbol) {
            Ok(())
        } else {
            Err(CcError::new(
                self.peek().span,
                format!("expected `{}`", symbol.text()),
            ))
        }
    }
}
