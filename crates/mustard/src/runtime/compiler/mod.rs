mod assignments;
mod bindings;
mod context;
mod control;
mod expressions;
mod statements;

use std::collections::HashSet;

use bindings::collect_block_bindings;
use context::CompileContext;

use crate::{
    diagnostic::MustardResult,
    ir::{CompiledProgram, FunctionExpr, Pattern, Stmt},
    span::SourceSpan,
};

use super::{
    bytecode::{BytecodeProgram, FunctionPrototype, Instruction},
    validation::validate_bytecode_program,
};

pub(super) fn pattern_bindings(pattern: &Pattern) -> Vec<(String, bool)> {
    bindings::pattern_bindings(pattern)
}

pub fn lower_to_bytecode(program: &CompiledProgram) -> MustardResult<BytecodeProgram> {
    let mut compiler = Compiler::default();
    let root = compiler.compile_root(&program.script.body, program.script.span)?;
    let program = BytecodeProgram {
        functions: compiler.functions,
        root,
    };
    validate_bytecode_program(&program)?;
    Ok(program)
}

#[derive(Debug, Default)]
struct Compiler {
    functions: Vec<FunctionPrototype>,
}

impl Compiler {
    fn compile_root(&mut self, statements: &[Stmt], span: SourceSpan) -> MustardResult<usize> {
        let mut context = CompileContext::default();
        self.emit_block_prologue(&mut context, statements, true)?;
        let mut produced_result = false;
        for (index, statement) in statements.iter().enumerate() {
            let is_last = index + 1 == statements.len();
            if is_last && let Stmt::Expression { expression, .. } = statement {
                self.compile_expr(&mut context, expression)?;
                produced_result = true;
                continue;
            }
            self.compile_stmt(&mut context, statement)?;
        }
        if !produced_result {
            context.code.push(Instruction::PushUndefined);
        }
        context.code.push(Instruction::Return);
        let id = self.functions.len();
        self.functions.push(FunctionPrototype {
            name: None,
            length: 0,
            display_source: String::new(),
            params: Vec::new(),
            rest: None,
            code: context.code,
            is_async: false,
            is_arrow: false,
            span,
        });
        Ok(id)
    }

    fn compile_function(&mut self, function: &FunctionExpr) -> MustardResult<usize> {
        self.compile_function_body(function)
    }

    fn compile_function_body(&mut self, function: &FunctionExpr) -> MustardResult<usize> {
        let mut context = CompileContext::default();
        for statement in &function.param_init {
            if let Stmt::VariableDecl { declarators, .. } = statement {
                for declarator in declarators {
                    for (name, _) in pattern_bindings(&declarator.pattern) {
                        context.code.push(Instruction::DeclareName {
                            name,
                            mutable: true,
                        });
                    }
                    if let Some(initializer) = &declarator.initializer {
                        self.compile_expr(&mut context, initializer)?;
                    } else {
                        context.code.push(Instruction::PushUndefined);
                    }
                    self.compile_pattern_binding(&mut context, &declarator.pattern)?;
                }
            }
        }
        self.emit_block_prologue(&mut context, &function.body, false)?;
        for statement in &function.body {
            self.compile_stmt(&mut context, statement)?;
        }
        context.code.push(Instruction::PushUndefined);
        context.code.push(Instruction::Return);
        let id = self.functions.len();
        self.functions.push(FunctionPrototype {
            name: function.name.clone(),
            length: function.length,
            display_source: function.display_source.clone(),
            params: function.params.clone(),
            rest: function.rest.clone(),
            code: context.code,
            is_async: function.is_async,
            is_arrow: function.is_arrow,
            span: function.span,
        });
        Ok(id)
    }

    fn emit_block_prologue(
        &mut self,
        context: &mut CompileContext,
        statements: &[Stmt],
        root_scope: bool,
    ) -> MustardResult<()> {
        let mut declared = HashSet::new();
        let bindings = collect_block_bindings(statements);
        for (name, mutable) in bindings.lexicals {
            if declared.insert(name.clone()) {
                context
                    .code
                    .push(Instruction::DeclareName { name, mutable });
            }
        }
        for function in bindings.functions {
            let function_name = function.name.clone();
            if declared.insert(function_name.clone()) {
                context.code.push(Instruction::DeclareName {
                    name: function_name.clone(),
                    mutable: false,
                });
            }
            context.code.push(Instruction::MakeClosure {
                function_id: self.compile_function(&function.expr)?,
            });
            context
                .code
                .push(Instruction::InitializePattern(Pattern::Identifier {
                    span: function.expr.span,
                    name: function_name.clone(),
                }));
            if root_scope {
                context.code.push(Instruction::LoadGlobalObject);
                context
                    .code
                    .push(Instruction::LoadName(function_name.clone()));
                context.code.push(Instruction::SetPropStatic {
                    name: function_name,
                });
                context.code.push(Instruction::Pop);
            }
        }
        Ok(())
    }

    fn fresh_internal_name(&self, context: &mut CompileContext, prefix: &str) -> String {
        let name = format!("\0mustard_{prefix}_{}", context.internal_name_counter);
        context.internal_name_counter += 1;
        name
    }
}
