use super::{
    builtins,
    const_eval::evaluate as evaluate_constant,
    control_flow::may_reach_function_end,
    ir::*,
    symbols::{collect_labels, FunctionSignature},
};
use crate::cc::{ast::*, CcError, Span};
use std::collections::HashMap;

pub(super) fn resolve(
    ast: Program,
    globals: &HashMap<String, Type>,
    functions: &HashMap<String, FunctionSignature>,
) -> Result<AnalyzedProgram, CcError> {
    let resolved_globals = ast
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Global(global) => Some(ResolvedGlobal {
                name: global.name.clone(),
                value: global
                    .init
                    .as_ref()
                    .map(evaluate_constant)
                    .transpose()
                    .expect("constant initializers were validated")
                    .map(|constant| constant.value)
                    .unwrap_or(0),
            }),
            Item::Function(_) => None,
        })
        .collect();
    let mut resolved_functions = Vec::new();
    for item in &ast.items {
        let Item::Function(function) = item else {
            continue;
        };
        let Some(body) = &function.body else {
            continue;
        };
        let labels = collect_labels(body)?;
        let mut resolver = Resolver {
            globals,
            functions,
            parameters: function
                .params
                .iter()
                .enumerate()
                .map(|(index, parameter)| {
                    (parameter.name.clone(), (ParamIndex(index), parameter.ty))
                })
                .collect(),
            scopes: Vec::new(),
            next_local: 0,
            return_type: function.ret,
            labels,
            break_depth: 0,
            continue_depth: 0,
        };
        let body = resolver.statement(body)?;
        let local_count = resolver.next_local;
        if function.ret != Type::Void && may_reach_function_end(&body) {
            return Err(CcError::new(
                function.span,
                format!(
                    "non-void function `{}` may reach the end without returning a value",
                    function.name
                ),
            ));
        }
        resolved_functions.push(ResolvedFunction {
            name: function.name.clone(),
            span: function.span,
            body,
            local_count,
            parameter_count: function.params.len(),
        });
    }
    Ok(AnalyzedProgram {
        globals: resolved_globals,
        functions: resolved_functions,
    })
}

struct Resolver<'a> {
    globals: &'a HashMap<String, Type>,
    functions: &'a HashMap<String, FunctionSignature>,
    parameters: HashMap<String, (ParamIndex, Type)>,
    scopes: Vec<HashMap<String, (LocalSlot, Type)>>,
    next_local: usize,
    return_type: Type,
    labels: HashMap<String, UserLabel>,
    break_depth: usize,
    continue_depth: usize,
}

impl Resolver<'_> {
    fn variable(&self, name: &str, span: Span) -> Result<(ResolvedVariable, Type), CcError> {
        for scope in self.scopes.iter().rev() {
            if let Some(&(slot, ty)) = scope.get(name) {
                return Ok((ResolvedVariable::Local(slot), ty));
            }
        }
        if let Some(&(index, ty)) = self.parameters.get(name) {
            return Ok((ResolvedVariable::Parameter(index), ty));
        }
        self.globals
            .get(name)
            .copied()
            .map(|ty| (ResolvedVariable::Global(GlobalSymbol(name.to_owned())), ty))
            .ok_or_else(|| CcError::new(span, format!("undeclared identifier `{name}`")))
    }

    fn expression(&mut self, expression: &Expr) -> Result<ResolvedExpr, CcError> {
        let (kind, ty) = match &expression.kind {
            ExprKind::Integer(literal) => (
                ResolvedExprKind::Number(literal.value),
                Type::Scalar(literal.ty()),
            ),
            ExprKind::Variable { name } => {
                let (variable, ty) = self.variable(name, expression.span)?;
                (ResolvedExprKind::Load(variable), ty)
            }
            ExprKind::Assign {
                target: name,
                value,
            } => {
                let value = self.expression(value)?;
                require_scalar(value.ty, expression.span)?;
                let (target, ty) = self.variable(name, expression.span)?;
                (
                    ResolvedExprKind::Assign {
                        target,
                        value: Box::new(value),
                    },
                    ty,
                )
            }
            ExprKind::Call {
                function: name,
                arguments,
            } => {
                let signature = self.functions.get(name).ok_or_else(|| {
                    CcError::new(
                        expression.span,
                        format!("call to undeclared function `{name}`"),
                    )
                })?;
                if arguments.len() != signature.params.len() {
                    return Err(CcError::new(
                        expression.span,
                        format!(
                            "function `{name}` expects {} arguments, got {}",
                            signature.params.len(),
                            arguments.len()
                        ),
                    ));
                }
                let mut resolved = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    let argument = self.expression(argument)?;
                    require_scalar(argument.ty, expression.span)?;
                    resolved.push(argument);
                }
                (
                    ResolvedExprKind::Call {
                        callee: builtins::find(name)
                            .map(|builtin| ResolvedCallee::Builtin(builtin.id))
                            .unwrap_or_else(|| ResolvedCallee::User(name.clone())),
                        args: resolved,
                    },
                    signature.ret,
                )
            }
            ExprKind::Unary { op, operand } => {
                let operand = self.expression(operand)?;
                require_scalar(operand.ty, expression.span)?;
                let ty = if *op == UnOp::Not {
                    Type::INT
                } else {
                    operand.ty
                };
                (
                    ResolvedExprKind::Unary {
                        op: *op,
                        operand: Box::new(operand),
                    },
                    ty,
                )
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let lhs = self.expression(lhs)?;
                let rhs = self.expression(rhs)?;
                let lhs_type = require_scalar(lhs.ty, expression.span)?;
                let rhs_type = require_scalar(rhs.ty, expression.span)?;
                let comparison = matches!(
                    op,
                    BinOp::Lt
                        | BinOp::Le
                        | BinOp::Gt
                        | BinOp::Ge
                        | BinOp::Eq
                        | BinOp::Ne
                        | BinOp::And
                        | BinOp::Or
                );
                let operand_type =
                    if lhs_type == ScalarType::UInt32 || rhs_type == ScalarType::UInt32 {
                        ScalarType::UInt32
                    } else {
                        ScalarType::Int32
                    };
                let ty = if comparison {
                    Type::INT
                } else {
                    Type::Scalar(operand_type)
                };
                (
                    ResolvedExprKind::Binary {
                        op: *op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                        operand_type,
                    },
                    ty,
                )
            }
        };
        Ok(ResolvedExpr { kind, ty })
    }

    fn statement(&mut self, statement: &Stmt) -> Result<ResolvedStmt, CcError> {
        Ok(match &statement.kind {
            StmtKind::Empty => ResolvedStmt::Empty,
            StmtKind::Expression { expression } => ResolvedStmt::Expr(self.expression(expression)?),
            StmtKind::Block { statements } => {
                self.scopes.push(HashMap::new());
                let result = statements
                    .iter()
                    .map(|statement| self.statement(statement))
                    .collect::<Result<Vec<_>, _>>();
                self.scopes.pop();
                ResolvedStmt::Block(result?)
            }
            StmtKind::Declaration {
                name,
                ty,
                initializer,
            } => {
                let top_level = self.scopes.len() == 1;
                let scope = self.scopes.last().ok_or_else(|| {
                    CcError::new(statement.span, "declaration outside a compound statement")
                })?;
                if scope.contains_key(name) || top_level && self.parameters.contains_key(name) {
                    return Err(CcError::new(
                        statement.span,
                        format!("redeclaration of `{name}`"),
                    ));
                }
                let initializer = initializer
                    .as_ref()
                    .map(|expression| self.expression(expression))
                    .transpose()?;
                if let Some(expression) = &initializer {
                    require_scalar(expression.ty, statement.span)?;
                }
                let slot = LocalSlot(self.next_local);
                self.next_local += 1;
                self.scopes
                    .last_mut()
                    .expect("scope existence was checked above")
                    .insert(name.clone(), (slot, *ty));
                ResolvedStmt::Decl {
                    slot,
                    init: initializer,
                }
            }
            StmtKind::If {
                condition,
                then_branch: then_stmt,
                else_branch: else_stmt,
            } => {
                let condition = self.expression(condition)?;
                require_scalar(condition.ty, statement.span)?;
                ResolvedStmt::If {
                    condition,
                    then_stmt: Box::new(self.statement(then_stmt)?),
                    else_stmt: else_stmt
                        .as_deref()
                        .map(|statement| self.statement(statement).map(Box::new))
                        .transpose()?,
                }
            }
            StmtKind::While { condition, body } => {
                let condition = self.expression(condition)?;
                require_scalar(condition.ty, statement.span)?;
                self.break_depth += 1;
                self.continue_depth += 1;
                let body = self.statement(body);
                self.break_depth -= 1;
                self.continue_depth -= 1;
                ResolvedStmt::While {
                    condition,
                    body: Box::new(body?),
                }
            }
            StmtKind::Switch { expression, parts } => {
                let expression = self.expression(expression)?;
                require_scalar(expression.ty, statement.span)?;
                self.break_depth += 1;
                self.scopes.push(HashMap::new());
                let result = parts
                    .iter()
                    .map(|part| match part {
                        SwitchPart::Case { value, .. } => Ok(ResolvedSwitchPart::Case(
                            evaluate_constant(value)
                                .expect("case expressions were validated")
                                .value,
                        )),
                        SwitchPart::Default { .. } => Ok(ResolvedSwitchPart::Default),
                        SwitchPart::Statement { statement } => {
                            self.statement(statement).map(ResolvedSwitchPart::Stmt)
                        }
                    })
                    .collect::<Result<Vec<_>, CcError>>();
                self.scopes.pop();
                self.break_depth -= 1;
                ResolvedStmt::Switch {
                    expression,
                    parts: result?,
                }
            }
            StmtKind::Break => {
                if self.break_depth == 0 {
                    return Err(CcError::new(
                        statement.span,
                        "`break` is not inside while or switch",
                    ));
                }
                ResolvedStmt::Break
            }
            StmtKind::Continue => {
                if self.continue_depth == 0 {
                    return Err(CcError::new(
                        statement.span,
                        "`continue` is not inside while",
                    ));
                }
                ResolvedStmt::Continue
            }
            StmtKind::Goto { label: name } => {
                ResolvedStmt::Goto(self.labels.get(name).copied().ok_or_else(|| {
                    CcError::new(statement.span, format!("undefined label `{name}`"))
                })?)
            }
            StmtKind::Label { name, body } => {
                ResolvedStmt::Label(self.labels[name], Box::new(self.statement(body)?))
            }
            StmtKind::Return { value: expression } => {
                let expression = expression
                    .as_ref()
                    .map(|expression| self.expression(expression))
                    .transpose()?;
                match (self.return_type, &expression) {
                    (Type::Void, None) => {}
                    (Type::Void, Some(_)) => {
                        return Err(CcError::new(
                            statement.span,
                            "void function cannot return a value",
                        ));
                    }
                    (_, None) => {
                        return Err(CcError::new(
                            statement.span,
                            "non-void function must return a value",
                        ));
                    }
                    (_, Some(expression)) => {
                        require_scalar(expression.ty, statement.span)?;
                    }
                }
                ResolvedStmt::Return(expression)
            }
        })
    }
}

fn require_scalar(ty: Type, span: Span) -> Result<ScalarType, CcError> {
    ty.scalar()
        .ok_or_else(|| CcError::new(span, "void expression used where a value is required"))
}
