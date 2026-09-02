use super::{
    builtins, const_eval::evaluate as evaluate_constant, control_flow::definitely_returns,
    UserLabel,
};
use crate::cc::{ast::*, is_implementation_reserved, CcError, Span};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub(super) struct FunctionSignature {
    pub ret: Type,
    pub params: Vec<Type>,
    pub defined: bool,
}

pub(super) struct SymbolTables {
    pub globals: HashMap<String, Type>,
    pub functions: HashMap<String, FunctionSignature>,
}

pub(super) fn collect(program: &Program) -> Result<SymbolTables, Vec<CcError>> {
    let mut globals = HashMap::new();
    let mut functions = builtins::signatures();
    let mut errors = Vec::new();

    for item in &program.items {
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

    for item in &program.items {
        if let Item::Function(function) = item {
            if let Some(body) = &function.body {
                validate_function(function, body, &mut errors);
            }
        }
    }

    if errors.is_empty() {
        Ok(SymbolTables { globals, functions })
    } else {
        Err(errors)
    }
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
    if count_locals(body) + function.params.len() + 3 > 32767 {
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
    if let Err(error) = collect_labels(body) {
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
            then_branch,
            else_branch,
            ..
        } => {
            validate_switches(then_branch, errors);
            if let Some(statement) = else_branch {
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
                    SwitchPart::Case { value, span } => match evaluate_constant(value) {
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

pub(super) fn count_locals(statement: &Stmt) -> usize {
    match &statement.kind {
        StmtKind::Declaration { .. } => 1,
        StmtKind::Block { statements } => statements.iter().map(count_locals).sum(),
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => count_locals(then_branch) + else_branch.as_deref().map(count_locals).unwrap_or(0),
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

fn is_reserved(name: &str) -> bool {
    builtins::find(name).is_some() || is_implementation_reserved(name)
}

pub(super) fn collect_labels(statement: &Stmt) -> Result<HashMap<String, UserLabel>, CcError> {
    fn walk(statement: &Stmt, labels: &mut HashMap<String, UserLabel>) -> Result<(), CcError> {
        match &statement.kind {
            StmtKind::Label { name, body } => {
                let next_id = labels.len();
                if labels.insert(name.clone(), UserLabel(next_id)).is_some() {
                    return Err(CcError::new(
                        statement.span,
                        format!("duplicate label `{name}`"),
                    ));
                }
                walk(body, labels)?;
            }
            StmtKind::Block { statements } => {
                for statement in statements {
                    walk(statement, labels)?;
                }
            }
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                walk(then_branch, labels)?;
                if let Some(statement) = else_branch {
                    walk(statement, labels)?;
                }
            }
            StmtKind::While { body, .. } => walk(body, labels)?,
            StmtKind::Switch { parts, .. } => {
                for part in parts {
                    if let SwitchPart::Statement { statement } = part {
                        walk(statement, labels)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    let mut labels = HashMap::new();
    walk(statement, &mut labels)?;
    Ok(labels)
}
