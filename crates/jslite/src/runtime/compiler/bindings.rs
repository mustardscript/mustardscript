use crate::{
    diagnostic::{JsliteError, JsliteResult},
    ir::{AssignOp, BinaryOp, BindingKind, FunctionExpr, Pattern, Stmt},
};

#[derive(Debug)]
pub(super) struct BlockBindings {
    pub(super) lexicals: Vec<(String, bool)>,
    pub(super) functions: Vec<FunctionBinding>,
}

#[derive(Debug)]
pub(super) struct FunctionBinding {
    pub(super) name: String,
    pub(super) expr: FunctionExpr,
}

pub(super) fn collect_block_bindings(statements: &[Stmt]) -> BlockBindings {
    let mut lexicals = Vec::new();
    let mut functions = Vec::new();
    for statement in statements {
        match statement {
            Stmt::VariableDecl {
                kind, declarators, ..
            } => {
                for declarator in declarators {
                    for (name, _) in pattern_bindings(&declarator.pattern) {
                        lexicals.push((name, *kind == BindingKind::Let));
                    }
                }
            }
            Stmt::FunctionDecl { function, .. } => {
                if let Some(name) = &function.name {
                    functions.push(FunctionBinding {
                        name: name.clone(),
                        expr: function.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    BlockBindings {
        lexicals,
        functions,
    }
}

pub(super) fn pattern_bindings(pattern: &Pattern) -> Vec<(String, bool)> {
    let mut bindings = Vec::new();
    collect_pattern_bindings(pattern, &mut bindings);
    bindings
}

fn collect_pattern_bindings(pattern: &Pattern, bindings: &mut Vec<(String, bool)>) {
    match pattern {
        Pattern::Identifier { name, .. } => bindings.push((name.clone(), true)),
        Pattern::Object {
            properties, rest, ..
        } => {
            for property in properties {
                collect_pattern_bindings(&property.value, bindings);
            }
            if let Some(rest) = rest {
                collect_pattern_bindings(rest, bindings);
            }
        }
        Pattern::Array { elements, rest, .. } => {
            for element in elements.iter().flatten() {
                collect_pattern_bindings(element, bindings);
            }
            if let Some(rest) = rest {
                collect_pattern_bindings(rest, bindings);
            }
        }
        Pattern::Default { target, .. } => collect_pattern_bindings(target, bindings),
    }
}

pub(super) fn assign_op_to_binary(operator: AssignOp) -> JsliteResult<BinaryOp> {
    match operator {
        AssignOp::Assign | AssignOp::OrAssign | AssignOp::AndAssign | AssignOp::NullishAssign => {
            Err(JsliteError::runtime("invalid compound assignment"))
        }
        AssignOp::AddAssign => Ok(BinaryOp::Add),
        AssignOp::SubAssign => Ok(BinaryOp::Sub),
        AssignOp::MulAssign => Ok(BinaryOp::Mul),
        AssignOp::DivAssign => Ok(BinaryOp::Div),
    }
}
