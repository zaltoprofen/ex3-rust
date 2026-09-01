use super::{ast::*, CcError, Span};
use std::collections::{HashMap, HashSet};

mod builtins;
mod const_eval;
mod control_flow;
use const_eval::evaluate as evaluate_constant;
use control_flow::definitely_returns;

#[derive(Clone)]
pub(crate) struct FunctionSignature {
    pub ret: Type,
    pub params: Vec<Type>,
    pub defined: bool,
}

pub(crate) struct AnalyzedProgram {
    pub globals: Vec<ResolvedGlobal>,
    pub functions: Vec<ResolvedFunction>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LocalSlot(pub usize);

#[derive(Clone, Copy, Debug)]
pub(crate) struct TempSlot(pub usize);

#[derive(Clone, Copy, Debug)]
pub(crate) struct ParamIndex(pub usize);

#[derive(Clone, Copy, Debug)]
pub(crate) struct StackOffset(pub usize);

#[derive(Clone, Copy, Debug)]
pub(crate) struct StackAdjustment(pub usize);

#[derive(Clone, Debug)]
pub(crate) struct GlobalSymbol(pub String);

#[derive(Clone, Debug)]
pub(crate) struct UserLabel(pub String);

#[derive(Clone, Debug)]
pub(crate) struct FunctionId {
    pub assembly_name: String,
    pub needs_runtime: bool,
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
        function: FunctionId,
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
    pub temporary_count: usize,
    pub parameter_count: usize,
}

pub(crate) fn analyze(ast: Program) -> Result<AnalyzedProgram, Vec<CcError>> {
    let mut globals = HashMap::new();
    let mut functions = builtins::signatures();
    let mut errors = Vec::new();

    for item in &ast.items {
        match item {
            Item::Global(global) => {
                if is_reserved(&global.name)
                    || globals.contains_key(&global.name)
                    || functions.contains_key(&global.name)
                {
                    errors.push(CcError::new(
                        global.span,
                        format!("duplicate or reserved global `{}`", global.name),
                    ));
                } else {
                    globals.insert(global.name.clone(), global.ty);
                }
                if let Some(initializer) = &global.init {
                    if let Err(message) = evaluate_constant(initializer) {
                        errors.push(CcError::new(initializer.span, message));
                    }
                }
            }
            Item::Function(function) => {
                collect_function(function, &globals, &mut functions, &mut errors)
            }
        }
    }

    match functions.get("main") {
        None => errors.push(CcError::new(
            Span { line: 1, column: 1 },
            "missing required `int main(void)` definition",
        )),
        Some(main) if main.ret != Type::INT || !main.params.is_empty() || !main.defined => {
            errors.push(CcError::new(
                Span { line: 1, column: 1 },
                "entry point must be defined as `int main(void)`",
            ));
        }
        Some(_) => {}
    }

    for item in &ast.items {
        if let Item::Function(function) = item {
            if let Some(body) = &function.body {
                validate_function(function, body, &mut errors);
            }
        }
    }

    if !errors.is_empty() {
        Err(errors)
    } else {
        resolve(ast, &globals, &functions).map_err(|error| vec![error])
    }
}

fn resolve(
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
        let labels = collect_labels(body, &function.name)?;
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
        resolved_functions.push(ResolvedFunction {
            name: function.name.clone(),
            body,
            local_count: count_locals(function.body.as_ref().unwrap()),
            temporary_count: max_temporaries_in_statement(function.body.as_ref().unwrap()).max(2),
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
                let builtin = builtins::find(name);
                (
                    ResolvedExprKind::Call {
                        function: FunctionId {
                            assembly_name: builtin
                                .map(|builtin| builtin.assembly_name)
                                .unwrap_or(name)
                                .to_owned(),
                            needs_runtime: builtin.is_some_and(|builtin| builtin.needs_runtime),
                        },
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
                if self.scopes.last().unwrap().contains_key(name)
                    || top_level && self.parameters.contains_key(name)
                {
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
                    .unwrap()
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
                ResolvedStmt::Goto(self.labels.get(name).cloned().ok_or_else(|| {
                    CcError::new(statement.span, format!("undefined label `{name}`"))
                })?)
            }
            StmtKind::Label { name, body } => {
                ResolvedStmt::Label(self.labels[name].clone(), Box::new(self.statement(body)?))
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

fn collect_function(
    function: &Function,
    globals: &HashMap<String, Type>,
    functions: &mut HashMap<String, FunctionSignature>,
    errors: &mut Vec<CcError>,
) {
    if builtins::find(&function.name).is_some() {
        let expected = &functions[&function.name];
        let params: Vec<_> = function.params.iter().map(|param| param.ty).collect();
        if function.body.is_some() || function.ret != expected.ret || params != expected.params {
            errors.push(CcError::new(
                function.span,
                format!(
                    "invalid redeclaration of reserved function `{}`",
                    function.name
                ),
            ));
        }
        return;
    }
    if is_reserved(&function.name) || globals.contains_key(&function.name) {
        errors.push(CcError::new(
            function.span,
            format!("duplicate or reserved function `{}`", function.name),
        ));
        return;
    }
    let signature = FunctionSignature {
        ret: function.ret,
        params: function.params.iter().map(|param| param.ty).collect(),
        defined: function.body.is_some(),
    };
    if let Some(previous) = functions.get_mut(&function.name) {
        if previous.ret != signature.ret
            || previous.params != signature.params
            || previous.defined && signature.defined
        {
            errors.push(CcError::new(
                function.span,
                format!(
                    "conflicting or duplicate declaration of `{}`",
                    function.name
                ),
            ));
        } else {
            previous.defined |= signature.defined;
        }
    } else {
        functions.insert(function.name.clone(), signature);
    }
}

fn validate_function(function: &Function, body: &Stmt, errors: &mut Vec<CcError>) {
    if function.ret != Type::Void && !definitely_returns(body) {
        errors.push(CcError::new(
            function.span,
            format!(
                "non-void function `{}` may reach the end without returning a value",
                function.name
            ),
        ));
    }
    let frame_size = count_locals(body) + max_temporaries_in_statement(body).max(2);
    if frame_size > 32767 || frame_size + function.params.len() + 1 > 32767 {
        errors.push(CcError::new(
            function.span,
            "function stack frame is too large",
        ));
    }
    let mut parameters = HashSet::new();
    for parameter in &function.params {
        if !parameters.insert(&parameter.name) {
            errors.push(CcError::new(
                parameter.span,
                format!("duplicate parameter `{}`", parameter.name),
            ));
        }
    }
    if let Err(error) = collect_labels(body, &function.name) {
        errors.push(error);
    }
    validate_switches(body, errors);
}

fn validate_switches(statement: &Stmt, errors: &mut Vec<CcError>) {
    match &statement.kind {
        StmtKind::Block { statements } => {
            for statement in statements {
                validate_switches(statement, errors);
            }
        }
        StmtKind::If {
            then_branch: then_statement,
            else_branch: else_statement,
            ..
        } => {
            validate_switches(then_statement, errors);
            if let Some(statement) = else_statement {
                validate_switches(statement, errors);
            }
        }
        StmtKind::While { body, .. } | StmtKind::Label { body, .. } => {
            validate_switches(body, errors)
        }
        StmtKind::Switch { parts, .. } => {
            let mut cases = HashSet::new();
            let mut has_default = false;
            for part in parts {
                match part {
                    SwitchPart::Case {
                        value: expression,
                        span,
                    } => match evaluate_constant(expression) {
                        Ok(constant) if !cases.insert(constant.value) => {
                            errors.push(CcError::new(*span, "duplicate case value"));
                        }
                        Err(message) => errors.push(CcError::new(*span, message)),
                        Ok(_) => {}
                    },
                    SwitchPart::Default { span } if has_default => {
                        errors.push(CcError::new(*span, "duplicate default label"));
                    }
                    SwitchPart::Default { .. } => has_default = true,
                    SwitchPart::Statement { statement } => validate_switches(statement, errors),
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn count_locals(statement: &Stmt) -> usize {
    match &statement.kind {
        StmtKind::Declaration { .. } => 1,
        StmtKind::Block { statements } => statements.iter().map(count_locals).sum(),
        StmtKind::If {
            then_branch: then_statement,
            else_branch: else_statement,
            ..
        } => {
            count_locals(then_statement) + else_statement.as_deref().map(count_locals).unwrap_or(0)
        }
        StmtKind::While { body, .. } | StmtKind::Label { body, .. } => count_locals(body),
        StmtKind::Switch { parts, .. } => parts
            .iter()
            .map(|part| match part {
                SwitchPart::Statement { statement } => count_locals(statement),
                _ => 0,
            })
            .sum(),
        _ => 0,
    }
}

fn max_temporaries_in_expression(expression: &Expr) -> usize {
    match &expression.kind {
        ExprKind::Unary { operand, .. } | ExprKind::Assign { value: operand, .. } => {
            max_temporaries_in_expression(operand)
        }
        ExprKind::Binary {
            op: BinOp::And | BinOp::Or,
            lhs,
            rhs,
        } => max_temporaries_in_expression(lhs).max(max_temporaries_in_expression(rhs)),
        ExprKind::Binary { lhs, rhs, .. } => max_temporaries_in_expression(lhs)
            .max(1 + max_temporaries_in_expression(rhs))
            .max(2),
        ExprKind::Call { arguments, .. } => arguments
            .iter()
            .map(max_temporaries_in_expression)
            .max()
            .unwrap_or(0),
        _ => 0,
    }
}

pub(crate) fn max_temporaries_in_statement(statement: &Stmt) -> usize {
    match &statement.kind {
        StmtKind::Expression { expression }
        | StmtKind::Return {
            value: Some(expression),
        } => max_temporaries_in_expression(expression),
        StmtKind::Declaration {
            initializer: Some(expression),
            ..
        } => max_temporaries_in_expression(expression),
        StmtKind::Block { statements } => statements
            .iter()
            .map(max_temporaries_in_statement)
            .max()
            .unwrap_or(0),
        StmtKind::If {
            condition: expression,
            then_branch: then_statement,
            else_branch: else_statement,
        } => max_temporaries_in_expression(expression)
            .max(max_temporaries_in_statement(then_statement))
            .max(
                else_statement
                    .as_deref()
                    .map(max_temporaries_in_statement)
                    .unwrap_or(0),
            ),
        StmtKind::While {
            condition: expression,
            body,
        } => max_temporaries_in_expression(expression).max(max_temporaries_in_statement(body)),
        StmtKind::Switch { expression, parts } => {
            1.max(max_temporaries_in_expression(expression)).max(
                parts
                    .iter()
                    .map(|part| match part {
                        SwitchPart::Statement { statement } => {
                            max_temporaries_in_statement(statement)
                        }
                        _ => 0,
                    })
                    .max()
                    .unwrap_or(0),
            )
        }
        StmtKind::Label { body, .. } => max_temporaries_in_statement(body),
        _ => 0,
    }
}

fn is_reserved(name: &str) -> bool {
    builtins::find(name).is_some() || name.starts_with("__cc_") || name.starts_with("__ex3_")
}

pub(crate) fn collect_labels(
    statement: &Stmt,
    function_name: &str,
) -> Result<HashMap<String, UserLabel>, CcError> {
    fn walk(
        statement: &Stmt,
        labels: &mut HashMap<String, UserLabel>,
        function_name: &str,
    ) -> Result<(), CcError> {
        match &statement.kind {
            StmtKind::Label { name, body } => {
                if labels
                    .insert(
                        name.clone(),
                        UserLabel(format!("__cc_user_{function_name}_{name}")),
                    )
                    .is_some()
                {
                    return Err(CcError::new(
                        statement.span,
                        format!("duplicate label `{name}`"),
                    ));
                }
                walk(body, labels, function_name)?;
            }
            StmtKind::Block { statements } => {
                for statement in statements {
                    walk(statement, labels, function_name)?;
                }
            }
            StmtKind::If {
                then_branch: then_statement,
                else_branch: else_statement,
                ..
            } => {
                walk(then_statement, labels, function_name)?;
                if let Some(statement) = else_statement {
                    walk(statement, labels, function_name)?;
                }
            }
            StmtKind::While { body, .. } => walk(body, labels, function_name)?,
            StmtKind::Switch { parts, .. } => {
                for part in parts {
                    if let SwitchPart::Statement { statement } = part {
                        walk(statement, labels, function_name)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    let mut labels = HashMap::new();
    walk(statement, &mut labels, function_name)?;
    Ok(labels)
}
