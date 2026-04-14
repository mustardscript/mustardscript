mod assignments;
mod bindings;
mod context;
mod control;
mod expressions;
mod statements;

use std::collections::HashSet;
use std::env;

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

#[derive(Debug, Clone, Copy, Default)]
struct BytecodeOptimizerConfig {
    disable_discard_peephole: bool,
    disable_stack_noop_peephole: bool,
}

impl Compiler {
    fn compile_root(&mut self, statements: &[Stmt], span: SourceSpan) -> MustardResult<usize> {
        let mut context = CompileContext::default();
        context.push_binding_scope();
        context.declare_binding("this".to_string());
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
        let code = Self::optimize_code(context.code);
        let id = self.functions.len();
        self.functions.push(FunctionPrototype {
            name: None,
            length: 0,
            display_source: String::new(),
            params: Vec::new(),
            param_binding_names: Vec::new(),
            rest: None,
            rest_binding_names: Vec::new(),
            code,
            is_async: false,
            is_arrow: false,
            span,
        });
        Ok(id)
    }

    fn compile_function(
        &mut self,
        parent_context: &CompileContext,
        function: &FunctionExpr,
    ) -> MustardResult<usize> {
        self.compile_function_body(parent_context, function)
    }

    fn compile_function_body(
        &mut self,
        parent_context: &CompileContext,
        function: &FunctionExpr,
    ) -> MustardResult<usize> {
        let mut context = CompileContext::with_inherited_bindings(&parent_context.binding_scopes);
        context.push_binding_scope();
        context.declare_binding("this".to_string());
        for pattern in &function.params {
            let Pattern::Identifier { name, .. } = pattern else {
                unreachable!("lowered function params should be identifier temporaries");
            };
            context.declare_binding(name.clone());
        }
        if let Some(Pattern::Identifier { name, .. }) = &function.rest {
            context.declare_binding(name.clone());
        }
        for statement in &function.param_init {
            if let Stmt::VariableDecl { declarators, .. } = statement {
                for declarator in declarators {
                    for (name, _) in pattern_bindings(&declarator.pattern) {
                        self.emit_declare_name(&mut context, name, true);
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
        let code = Self::optimize_code(context.code);
        let id = self.functions.len();
        self.functions.push(FunctionPrototype {
            name: function.name.clone(),
            length: function.length,
            display_source: function.display_source.clone(),
            params: function.params.clone(),
            param_binding_names: function
                .params
                .iter()
                .map(|pattern| {
                    pattern_bindings(pattern)
                        .into_iter()
                        .map(|(name, _)| name)
                        .collect()
                })
                .collect(),
            rest: function.rest.clone(),
            rest_binding_names: function
                .rest
                .iter()
                .flat_map(|pattern| pattern_bindings(pattern).into_iter().map(|(name, _)| name))
                .collect(),
            code,
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
                self.emit_declare_name(context, name, mutable);
            }
        }
        for function in bindings.functions {
            let function_name = function.name.clone();
            if declared.insert(function_name.clone()) {
                self.emit_declare_name(context, function_name.clone(), false);
            }
            context.code.push(Instruction::MakeClosure {
                function_id: self.compile_function(context, &function.expr)?,
            });
            context
                .code
                .push(Instruction::InitializePattern(Pattern::Identifier {
                    span: function.expr.span,
                    name: function_name.clone(),
                }));
            if root_scope {
                context.code.push(Instruction::LoadGlobalObject);
                self.emit_load_name(context, &function_name);
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

    fn enter_env_scope(&self, context: &mut CompileContext) {
        context.code.push(Instruction::PushEnv);
        context.scope_depth += 1;
        context.push_binding_scope();
    }

    fn exit_env_scope(&self, context: &mut CompileContext) {
        context.scope_depth -= 1;
        context.pop_binding_scope();
        context.code.push(Instruction::PopEnv);
    }

    fn emit_declare_name(&self, context: &mut CompileContext, name: String, mutable: bool) {
        context.declare_binding(name.clone());
        context
            .code
            .push(Instruction::DeclareName { name, mutable });
    }

    fn emit_load_name(&self, context: &mut CompileContext, name: &str) {
        if let Some(binding) = context.resolve_binding(name) {
            context.code.push(Instruction::LoadSlot {
                depth: binding.depth,
                slot: binding.slot,
            });
        } else {
            context.code.push(Instruction::LoadGlobal(name.to_string()));
        }
    }

    fn emit_store_name(&self, context: &mut CompileContext, name: &str) {
        if let Some(binding) = context.resolve_binding(name) {
            context.code.push(Instruction::StoreSlot {
                depth: binding.depth,
                slot: binding.slot,
            });
        } else {
            context
                .code
                .push(Instruction::StoreGlobal(name.to_string()));
        }
    }

    fn emit_store_name_discard(&self, context: &mut CompileContext, name: &str) {
        if let Some(binding) = context.resolve_binding(name) {
            context.code.push(Instruction::StoreSlotDiscard {
                depth: binding.depth,
                slot: binding.slot,
            });
        } else {
            context
                .code
                .push(Instruction::StoreGlobalDiscard(name.to_string()));
        }
    }

    fn optimize_code(code: Vec<Instruction>) -> Vec<Instruction> {
        let config = Self::optimizer_config();
        let code = if config.disable_discard_peephole {
            code
        } else {
            Self::apply_discard_peephole(code)
        };
        if config.disable_stack_noop_peephole {
            code
        } else {
            Self::apply_stack_noop_peephole(code)
        }
    }

    fn optimizer_config() -> BytecodeOptimizerConfig {
        BytecodeOptimizerConfig {
            disable_discard_peephole: Self::env_flag_enabled(
                "MUSTARD_DISABLE_BYTECODE_DISCARD_PEEPHOLE",
            ),
            disable_stack_noop_peephole: Self::env_flag_enabled(
                "MUSTARD_DISABLE_BYTECODE_STACK_NOOP_PEEPHOLE",
            ),
        }
    }

    fn env_flag_enabled(name: &str) -> bool {
        env::var(name).is_ok_and(|value| {
            !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
        })
    }

    fn apply_discard_peephole(code: Vec<Instruction>) -> Vec<Instruction> {
        if !code.windows(2).any(|window| {
            matches!(window[1], Instruction::Pop) && Self::supports_discard_peephole(&window[0])
        }) {
            return code;
        }

        let protected_targets = Self::protected_jump_targets(&code);
        if !code.windows(2).enumerate().any(|(index, window)| {
            matches!(window[1], Instruction::Pop)
                && !protected_targets[index + 1]
                && Self::supports_discard_peephole(&window[0])
        }) {
            return code;
        }

        let mut old_to_new = vec![0; code.len() + 1];
        let mut optimized = Vec::with_capacity(code.len());
        let mut index = 0;
        while index < code.len() {
            old_to_new[index] = optimized.len();
            if let Some(instruction) =
                Self::rewrite_discard_peephole(&code, index, &protected_targets)
            {
                optimized.push(instruction);
                old_to_new[index + 1] = optimized.len();
                index += 2;
                continue;
            }
            optimized.push(code[index].clone());
            index += 1;
        }
        old_to_new[code.len()] = optimized.len();
        for instruction in &mut optimized {
            Self::remap_targets(instruction, &old_to_new);
        }
        optimized
    }

    fn apply_stack_noop_peephole(code: Vec<Instruction>) -> Vec<Instruction> {
        if !code
            .windows(2)
            .any(|window| matches!(window, [Instruction::Dup, Instruction::Pop]))
            && !code.windows(3).any(|window| {
                matches!(
                    window,
                    [Instruction::Dup2, Instruction::Pop, Instruction::Pop]
                )
            })
        {
            return code;
        }

        let protected_targets = Self::protected_jump_targets(&code);
        let mut old_to_new = vec![0; code.len() + 1];
        let mut optimized = Vec::with_capacity(code.len());
        let mut index = 0;
        while index < code.len() {
            old_to_new[index] = optimized.len();
            if Self::can_remove_dup_pop(&code, index, &protected_targets) {
                old_to_new[index + 1] = optimized.len();
                index += 2;
                continue;
            }
            if Self::can_remove_dup2_pop_pop(&code, index, &protected_targets) {
                old_to_new[index + 1] = optimized.len();
                old_to_new[index + 2] = optimized.len();
                index += 3;
                continue;
            }
            optimized.push(code[index].clone());
            index += 1;
        }
        old_to_new[code.len()] = optimized.len();
        for instruction in &mut optimized {
            Self::remap_targets(instruction, &old_to_new);
        }
        optimized
    }

    fn can_remove_dup_pop(code: &[Instruction], index: usize, protected_targets: &[bool]) -> bool {
        let next = index + 1;
        next < code.len()
            && !protected_targets[index]
            && !protected_targets[next]
            && matches!(code[index], Instruction::Dup)
            && matches!(code[next], Instruction::Pop)
    }

    fn can_remove_dup2_pop_pop(
        code: &[Instruction],
        index: usize,
        protected_targets: &[bool],
    ) -> bool {
        let next = index + 1;
        let tail = index + 2;
        tail < code.len()
            && !protected_targets[index]
            && !protected_targets[next]
            && !protected_targets[tail]
            && matches!(code[index], Instruction::Dup2)
            && matches!(code[next], Instruction::Pop)
            && matches!(code[tail], Instruction::Pop)
    }

    fn protected_jump_targets(code: &[Instruction]) -> Vec<bool> {
        let mut targets = vec![false; code.len()];
        for instruction in code {
            match instruction {
                Instruction::Jump(target)
                | Instruction::JumpIfFalse(target)
                | Instruction::JumpIfTrue(target)
                | Instruction::JumpIfNullish(target)
                | Instruction::EnterFinally { exit: target }
                | Instruction::PushPendingJump { target, .. } => {
                    targets[*target] = true;
                }
                Instruction::PushHandler { catch, finally } => {
                    if let Some(target) = catch {
                        targets[*target] = true;
                    }
                    if let Some(target) = finally {
                        targets[*target] = true;
                    }
                }
                _ => {}
            }
        }
        targets
    }

    fn supports_discard_peephole(instruction: &Instruction) -> bool {
        matches!(
            instruction,
            Instruction::StoreSlot { .. }
                | Instruction::StoreName(_)
                | Instruction::StoreGlobal(_)
                | Instruction::SetPropStatic { .. }
                | Instruction::SetPropComputed
        )
    }

    fn rewrite_discard_peephole(
        code: &[Instruction],
        index: usize,
        protected_targets: &[bool],
    ) -> Option<Instruction> {
        let next = index + 1;
        if next >= code.len() || !matches!(code[next], Instruction::Pop) || protected_targets[next]
        {
            return None;
        }
        match &code[index] {
            Instruction::StoreSlot { depth, slot } => Some(Instruction::StoreSlotDiscard {
                depth: *depth,
                slot: *slot,
            }),
            Instruction::StoreName(name) => Some(Instruction::StoreNameDiscard(name.clone())),
            Instruction::StoreGlobal(name) => Some(Instruction::StoreGlobalDiscard(name.clone())),
            Instruction::SetPropStatic { name } => {
                Some(Instruction::SetPropStaticDiscard { name: name.clone() })
            }
            Instruction::SetPropComputed => Some(Instruction::SetPropComputedDiscard),
            _ => None,
        }
    }

    fn remap_targets(instruction: &mut Instruction, old_to_new: &[usize]) {
        match instruction {
            Instruction::Jump(target)
            | Instruction::JumpIfFalse(target)
            | Instruction::JumpIfTrue(target)
            | Instruction::JumpIfNullish(target)
            | Instruction::EnterFinally { exit: target }
            | Instruction::PushPendingJump { target, .. } => {
                *target = old_to_new[*target];
            }
            Instruction::PushHandler { catch, finally } => {
                if let Some(target) = catch {
                    *target = old_to_new[*target];
                }
                if let Some(target) = finally {
                    *target = old_to_new[*target];
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_noop_peephole_removes_dup_pop_pairs() {
        let optimized = Compiler::apply_stack_noop_peephole(vec![
            Instruction::PushNumber(1.0),
            Instruction::Dup,
            Instruction::Pop,
            Instruction::Return,
        ]);

        assert!(matches!(
            optimized.as_slice(),
            [Instruction::PushNumber(1.0), Instruction::Return]
        ));
    }

    #[test]
    fn stack_noop_peephole_removes_dup2_pop_pop_triplets() {
        let optimized = Compiler::apply_stack_noop_peephole(vec![
            Instruction::PushNumber(1.0),
            Instruction::PushNumber(2.0),
            Instruction::Dup2,
            Instruction::Pop,
            Instruction::Pop,
            Instruction::Return,
        ]);

        assert!(matches!(
            optimized.as_slice(),
            [
                Instruction::PushNumber(1.0),
                Instruction::PushNumber(2.0),
                Instruction::Return,
            ]
        ));
    }

    #[test]
    fn stack_noop_peephole_preserves_targeted_sequences() {
        let optimized = Compiler::apply_stack_noop_peephole(vec![
            Instruction::Jump(1),
            Instruction::Dup,
            Instruction::Pop,
            Instruction::Return,
        ]);

        assert!(matches!(
            optimized.as_slice(),
            [
                Instruction::Jump(1),
                Instruction::Dup,
                Instruction::Pop,
                Instruction::Return,
            ]
        ));
    }
}
