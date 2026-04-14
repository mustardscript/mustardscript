mod assignments;
mod bindings;
mod context;
mod control;
mod expressions;
mod statements;

use std::collections::{HashMap, HashSet};
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
    enable_top_of_stack_peephole: bool,
    disable_stack_noop_peephole: bool,
    disable_superinstruction_peephole: bool,
}

type BlockLocalRewrite = fn(&[Instruction]) -> (Vec<Instruction>, Vec<usize>);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum AbstractBinding {
    Slot { depth: usize, slot: usize },
    Name(String),
    Global(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum AbstractValue {
    Undefined,
    Null,
    Bool(bool),
    Number(u64),
    String(String),
    BigInt(String),
    GlobalObject,
    Binding(AbstractBinding),
    Temporary(u64),
}

#[derive(Debug, Default)]
struct TopOfStackState {
    stack: Vec<AbstractValue>,
    slots: HashMap<(usize, usize), AbstractValue>,
    names: HashMap<String, AbstractValue>,
    globals: HashMap<String, AbstractValue>,
    next_temporary: u64,
}

impl TopOfStackState {
    fn fresh_temporary(&mut self) -> AbstractValue {
        let id = self.next_temporary;
        self.next_temporary += 1;
        AbstractValue::Temporary(id)
    }

    fn pop_value(&mut self) -> AbstractValue {
        self.stack.pop().unwrap_or_else(|| self.fresh_temporary())
    }

    fn peek_value(&self) -> Option<&AbstractValue> {
        self.stack.last()
    }

    fn push_value(&mut self, value: AbstractValue) {
        self.stack.push(value);
    }

    fn push_temporary(&mut self) {
        let value = self.fresh_temporary();
        self.push_value(value);
    }

    fn truncate_stack(&mut self, len: usize) {
        self.stack.truncate(len);
    }

    fn binding_value(&self, binding: &AbstractBinding) -> AbstractValue {
        match binding {
            AbstractBinding::Slot { depth, slot } => self
                .slots
                .get(&(*depth, *slot))
                .cloned()
                .unwrap_or_else(|| AbstractValue::Binding(binding.clone())),
            AbstractBinding::Name(name) => self
                .names
                .get(name)
                .cloned()
                .unwrap_or_else(|| AbstractValue::Binding(binding.clone())),
            AbstractBinding::Global(name) => self
                .globals
                .get(name)
                .cloned()
                .unwrap_or_else(|| AbstractValue::Binding(binding.clone())),
        }
    }

    fn record_binding(&mut self, binding: AbstractBinding, value: AbstractValue) {
        match binding {
            AbstractBinding::Slot { depth, slot } => {
                self.slots.insert((depth, slot), value);
            }
            AbstractBinding::Name(name) => {
                self.names.insert(name, value);
            }
            AbstractBinding::Global(name) => {
                self.globals.insert(name, value);
            }
        }
    }

    fn invalidate_locals(&mut self) {
        self.slots.clear();
        self.names.clear();
    }
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
        let code = if config.enable_top_of_stack_peephole {
            Self::apply_top_of_stack_peephole(code)
        } else {
            code
        };
        let code = if config.disable_stack_noop_peephole {
            code
        } else {
            Self::apply_stack_noop_peephole(code)
        };
        if config.disable_superinstruction_peephole {
            code
        } else {
            Self::apply_superinstruction_peephole(code)
        }
    }

    fn optimizer_config() -> BytecodeOptimizerConfig {
        BytecodeOptimizerConfig {
            disable_discard_peephole: Self::env_flag_enabled(
                "MUSTARD_DISABLE_BYTECODE_DISCARD_PEEPHOLE",
            ),
            enable_top_of_stack_peephole: Self::env_flag_enabled(
                "MUSTARD_ENABLE_BYTECODE_TOP_OF_STACK_PEEPHOLE",
            ),
            disable_stack_noop_peephole: Self::env_flag_enabled(
                "MUSTARD_DISABLE_BYTECODE_STACK_NOOP_PEEPHOLE",
            ),
            disable_superinstruction_peephole: Self::env_flag_enabled(
                "MUSTARD_DISABLE_BYTECODE_SUPERINSTRUCTION_PEEPHOLE",
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

    fn apply_top_of_stack_peephole(code: Vec<Instruction>) -> Vec<Instruction> {
        if !code.iter().any(Self::supports_top_of_stack_rewrite) {
            return code;
        }

        Self::apply_block_local_peephole(code, Self::rewrite_top_of_stack_block)
    }

    fn apply_superinstruction_peephole(code: Vec<Instruction>) -> Vec<Instruction> {
        if !code.windows(2).any(Self::supports_superinstruction_pair)
            && !code.windows(3).any(Self::supports_superinstruction_triplet)
        {
            return code;
        }

        Self::apply_block_local_peephole(code, Self::rewrite_superinstruction_block)
    }

    fn supports_top_of_stack_rewrite(instruction: &Instruction) -> bool {
        matches!(
            instruction,
            Instruction::PushUndefined
                | Instruction::PushNull
                | Instruction::PushBool(_)
                | Instruction::PushNumber(_)
                | Instruction::PushString(_)
                | Instruction::PushBigInt(_)
                | Instruction::LoadSlot { .. }
                | Instruction::LoadName(_)
                | Instruction::LoadGlobal(_)
                | Instruction::LoadGlobalObject
        )
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

    fn optimizer_block_starts(code: &[Instruction]) -> Vec<bool> {
        let mut starts = vec![false; code.len() + 1];
        starts[0] = true;
        let protected_targets = Self::protected_jump_targets(code);
        for (index, targeted) in protected_targets.into_iter().enumerate() {
            if targeted {
                starts[index] = true;
            }
        }
        for (index, instruction) in code.iter().enumerate() {
            if Self::optimizer_flush_after(instruction) {
                starts[index + 1] = true;
            }
        }
        starts
    }

    fn optimizer_flush_after(instruction: &Instruction) -> bool {
        matches!(
            instruction,
            Instruction::PushHandler { .. }
                | Instruction::PopHandler
                | Instruction::EnterFinally { .. }
                | Instruction::BeginCatch
                | Instruction::Throw { .. }
                | Instruction::PushPendingJump { .. }
                | Instruction::PushPendingReturn
                | Instruction::PushPendingThrow
                | Instruction::ContinuePending
                | Instruction::Jump(_)
                | Instruction::JumpIfFalse(_)
                | Instruction::JumpIfTrue(_)
                | Instruction::JumpIfNullish(_)
                | Instruction::Call { .. }
                | Instruction::CallWithArray { .. }
                | Instruction::Await
                | Instruction::Construct { .. }
                | Instruction::ConstructWithArray
                | Instruction::Return
        )
    }

    fn apply_block_local_peephole(
        code: Vec<Instruction>,
        rewrite_block: BlockLocalRewrite,
    ) -> Vec<Instruction> {
        let block_starts = Self::optimizer_block_starts(&code);
        let mut old_to_new = vec![0; code.len() + 1];
        let mut optimized = Vec::with_capacity(code.len());
        let mut block_start = 0;
        while block_start < code.len() {
            debug_assert!(block_starts[block_start]);
            let mut block_end = block_start + 1;
            while block_end < code.len() && !block_starts[block_end] {
                block_end += 1;
            }
            let base = optimized.len();
            let (rewritten, local_map) = rewrite_block(&code[block_start..block_end]);
            debug_assert_eq!(local_map.len(), block_end - block_start + 1);
            for (offset, mapped) in local_map.into_iter().enumerate() {
                old_to_new[block_start + offset] = base + mapped;
            }
            optimized.extend(rewritten);
            block_start = block_end;
        }
        old_to_new[code.len()] = optimized.len();
        for instruction in &mut optimized {
            Self::remap_targets(instruction, &old_to_new);
        }
        optimized
    }

    fn rewrite_top_of_stack_block(block: &[Instruction]) -> (Vec<Instruction>, Vec<usize>) {
        let mut state = TopOfStackState::default();
        let mut old_to_new = vec![0; block.len() + 1];
        let mut optimized = Vec::with_capacity(block.len());
        for (index, instruction) in block.iter().enumerate() {
            old_to_new[index] = optimized.len();
            if let Some(value) = Self::top_of_stack_candidate_value(&state, instruction)
                && state.peek_value().is_some_and(|top| *top == value)
            {
                optimized.push(Instruction::Dup);
                state.push_value(value);
                continue;
            }
            optimized.push(instruction.clone());
            Self::apply_top_of_stack_effect(&mut state, instruction);
        }
        old_to_new[block.len()] = optimized.len();
        (optimized, old_to_new)
    }

    fn top_of_stack_candidate_value(
        state: &TopOfStackState,
        instruction: &Instruction,
    ) -> Option<AbstractValue> {
        match instruction {
            Instruction::PushUndefined => Some(AbstractValue::Undefined),
            Instruction::PushNull => Some(AbstractValue::Null),
            Instruction::PushBool(value) => Some(AbstractValue::Bool(*value)),
            Instruction::PushNumber(value) => Some(AbstractValue::Number(value.to_bits())),
            Instruction::PushString(value) => Some(AbstractValue::String(value.clone())),
            Instruction::PushBigInt(value) => Some(AbstractValue::BigInt(value.clone())),
            Instruction::LoadSlot { depth, slot } => {
                Some(state.binding_value(&AbstractBinding::Slot {
                    depth: *depth,
                    slot: *slot,
                }))
            }
            Instruction::LoadName(name) => {
                Some(state.binding_value(&AbstractBinding::Name(name.clone())))
            }
            Instruction::LoadGlobal(name) => {
                Some(state.binding_value(&AbstractBinding::Global(name.clone())))
            }
            Instruction::LoadGlobalObject => Some(AbstractValue::GlobalObject),
            _ => None,
        }
    }

    fn apply_top_of_stack_effect(state: &mut TopOfStackState, instruction: &Instruction) {
        match instruction {
            Instruction::PushUndefined => state.push_value(AbstractValue::Undefined),
            Instruction::PushNull => state.push_value(AbstractValue::Null),
            Instruction::PushBool(value) => state.push_value(AbstractValue::Bool(*value)),
            Instruction::PushNumber(value) => {
                state.push_value(AbstractValue::Number(value.to_bits()))
            }
            Instruction::PushString(value) => {
                state.push_value(AbstractValue::String(value.clone()))
            }
            Instruction::PushBigInt(value) => {
                state.push_value(AbstractValue::BigInt(value.clone()))
            }
            Instruction::LoadSlot { depth, slot } => {
                let value = state.binding_value(&AbstractBinding::Slot {
                    depth: *depth,
                    slot: *slot,
                });
                state.push_value(value);
            }
            Instruction::LoadName(name) => {
                let value = state.binding_value(&AbstractBinding::Name(name.clone()));
                state.push_value(value);
            }
            Instruction::LoadGlobal(name) => {
                let value = state.binding_value(&AbstractBinding::Global(name.clone()));
                state.push_value(value);
            }
            Instruction::LoadGlobalObject => state.push_value(AbstractValue::GlobalObject),
            Instruction::StoreSlot { depth, slot } => {
                let value = state.pop_value();
                state.record_binding(
                    AbstractBinding::Slot {
                        depth: *depth,
                        slot: *slot,
                    },
                    value.clone(),
                );
                state.push_value(value);
            }
            Instruction::StoreName(name) => {
                let value = state.pop_value();
                state.record_binding(AbstractBinding::Name(name.clone()), value.clone());
                state.push_value(value);
            }
            Instruction::StoreGlobal(name) => {
                let value = state.pop_value();
                state.record_binding(AbstractBinding::Global(name.clone()), value.clone());
                state.push_value(value);
            }
            Instruction::StoreSlotDiscard { depth, slot } => {
                let value = state.pop_value();
                state.record_binding(
                    AbstractBinding::Slot {
                        depth: *depth,
                        slot: *slot,
                    },
                    value,
                );
            }
            Instruction::StoreNameDiscard(name) => {
                let value = state.pop_value();
                state.record_binding(AbstractBinding::Name(name.clone()), value);
            }
            Instruction::StoreGlobalDiscard(name) => {
                let value = state.pop_value();
                state.record_binding(AbstractBinding::Global(name.clone()), value);
            }
            Instruction::InitializePattern(_) => {
                state.pop_value();
                state.invalidate_locals();
            }
            Instruction::PushEnv | Instruction::PopEnv => {
                state.invalidate_locals();
            }
            Instruction::DeclareName { name, .. } => {
                state.names.remove(name);
            }
            Instruction::MakeClosure { .. } | Instruction::PushRegExp { .. } => {
                state.push_temporary()
            }
            Instruction::MakeArray { count } => {
                let len = state.stack.len().saturating_sub(*count);
                state.truncate_stack(len);
                state.push_temporary();
            }
            Instruction::ArrayPush => {
                state.pop_value();
                let target = state.pop_value();
                state.push_value(target);
            }
            Instruction::ArrayPushHole => {
                let target = state.pop_value();
                state.push_value(target);
            }
            Instruction::ArrayExtend => {
                state.pop_value();
            }
            Instruction::MakeObject { keys } => {
                let len = state.stack.len().saturating_sub(keys.len());
                state.truncate_stack(len);
                state.push_temporary();
            }
            Instruction::CopyDataProperties => {
                state.pop_value();
            }
            Instruction::CreateIterator => {
                state.pop_value();
                state.push_temporary();
            }
            Instruction::IteratorNext => {
                state.pop_value();
                state.push_temporary();
                state.push_temporary();
            }
            Instruction::GetPropStatic { .. }
            | Instruction::PatternArrayIndex(_)
            | Instruction::PatternArrayRest(_)
            | Instruction::PatternObjectRest(_)
            | Instruction::Unary(_)
            | Instruction::Update(_) => {
                state.pop_value();
                state.push_temporary();
            }
            Instruction::GetPropComputed { .. } => {
                state.pop_value();
                state.pop_value();
                state.push_temporary();
            }
            Instruction::SetPropStatic { .. } => {
                let value = state.pop_value();
                state.pop_value();
                state.push_value(value);
            }
            Instruction::SetPropComputed => {
                let value = state.pop_value();
                state.pop_value();
                state.pop_value();
                state.push_value(value);
            }
            Instruction::SetPropStaticDiscard { .. } => {
                state.pop_value();
                state.pop_value();
            }
            Instruction::SetPropComputedDiscard => {
                state.pop_value();
                state.pop_value();
                state.pop_value();
            }
            Instruction::Binary(_) => {
                state.pop_value();
                state.pop_value();
                state.push_temporary();
            }
            Instruction::Pop => {
                state.pop_value();
            }
            Instruction::Dup => {
                let value = match state.peek_value().cloned() {
                    Some(value) => value,
                    None => state.fresh_temporary(),
                };
                state.push_value(value);
            }
            Instruction::Dup2 => {
                let len = state.stack.len();
                let first = match state.stack.get(len.saturating_sub(2)).cloned() {
                    Some(value) => value,
                    None => state.fresh_temporary(),
                };
                let second = match state.stack.get(len.saturating_sub(1)).cloned() {
                    Some(value) => value,
                    None => state.fresh_temporary(),
                };
                state.push_value(first);
                state.push_value(second);
            }
            Instruction::PushHandler { .. }
            | Instruction::PopHandler
            | Instruction::EnterFinally { .. }
            | Instruction::Jump(_)
            | Instruction::JumpIfFalse(_)
            | Instruction::JumpIfTrue(_)
            | Instruction::JumpIfNullish(_)
            | Instruction::ContinuePending
            | Instruction::Return => {}
            Instruction::BeginCatch => {
                state.push_temporary();
            }
            Instruction::Throw { .. }
            | Instruction::PushPendingReturn
            | Instruction::PushPendingThrow => {
                state.pop_value();
            }
            Instruction::PushPendingJump { .. } => {}
            Instruction::Call {
                argc, with_this, ..
            } => {
                let arg_len = state.stack.len().saturating_sub(*argc);
                state.truncate_stack(arg_len);
                state.pop_value();
                if *with_this {
                    state.pop_value();
                }
                state.push_temporary();
            }
            Instruction::CallWithArray { with_this, .. } => {
                state.pop_value();
                state.pop_value();
                if *with_this {
                    state.pop_value();
                }
                state.push_temporary();
            }
            Instruction::Await => {
                state.pop_value();
                state.push_temporary();
            }
            Instruction::Construct { argc } => {
                let arg_len = state.stack.len().saturating_sub(*argc);
                state.truncate_stack(arg_len);
                state.pop_value();
                state.push_temporary();
            }
            Instruction::ConstructWithArray => {
                state.pop_value();
                state.pop_value();
                state.push_temporary();
            }
            Instruction::LoadSlotGetPropStatic { .. } | Instruction::DupGetPropStatic { .. } => {
                state.push_temporary();
            }
            Instruction::LoadSlotDupGetPropStatic { .. } => {
                state.push_temporary();
                state.push_temporary();
            }
        }
    }

    fn supports_superinstruction_pair(window: &[Instruction]) -> bool {
        matches!(
            window,
            [
                Instruction::LoadSlot { .. },
                Instruction::GetPropStatic { .. }
            ] | [Instruction::Dup, Instruction::GetPropStatic { .. }]
        )
    }

    fn supports_superinstruction_triplet(window: &[Instruction]) -> bool {
        matches!(
            window,
            [
                Instruction::LoadSlot { .. },
                Instruction::Dup,
                Instruction::GetPropStatic { .. },
            ]
        )
    }

    fn rewrite_superinstruction_block(block: &[Instruction]) -> (Vec<Instruction>, Vec<usize>) {
        let mut old_to_new = vec![0; block.len() + 1];
        let mut optimized = Vec::with_capacity(block.len());
        let mut index = 0;
        while index < block.len() {
            old_to_new[index] = optimized.len();
            if let Some(instruction) = Self::rewrite_superinstruction_triplet(block, index) {
                optimized.push(instruction);
                old_to_new[index + 1] = optimized.len();
                old_to_new[index + 2] = optimized.len();
                index += 3;
                continue;
            }
            if let Some(instruction) = Self::rewrite_superinstruction_pair(block, index) {
                optimized.push(instruction);
                old_to_new[index + 1] = optimized.len();
                index += 2;
                continue;
            }
            optimized.push(block[index].clone());
            index += 1;
        }
        old_to_new[block.len()] = optimized.len();
        (optimized, old_to_new)
    }

    fn rewrite_superinstruction_pair(block: &[Instruction], index: usize) -> Option<Instruction> {
        let next = block.get(index + 1)?;
        match (&block[index], next) {
            (
                Instruction::LoadSlot { depth, slot },
                Instruction::GetPropStatic { name, optional },
            ) => Some(Instruction::LoadSlotGetPropStatic {
                depth: *depth,
                slot: *slot,
                name: name.clone(),
                optional: *optional,
            }),
            (Instruction::Dup, Instruction::GetPropStatic { name, optional }) => {
                Some(Instruction::DupGetPropStatic {
                    name: name.clone(),
                    optional: *optional,
                })
            }
            _ => None,
        }
    }

    fn rewrite_superinstruction_triplet(
        block: &[Instruction],
        index: usize,
    ) -> Option<Instruction> {
        match block.get(index..index + 3)? {
            [
                Instruction::LoadSlot { depth, slot },
                Instruction::Dup,
                Instruction::GetPropStatic { name, optional },
            ] => Some(Instruction::LoadSlotDupGetPropStatic {
                depth: *depth,
                slot: *slot,
                name: name.clone(),
                optional: *optional,
            }),
            _ => None,
        }
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

    #[test]
    fn superinstruction_peephole_fuses_load_slot_get_prop_static_pairs() {
        let optimized = Compiler::apply_superinstruction_peephole(vec![
            Instruction::LoadSlot { depth: 0, slot: 1 },
            Instruction::GetPropStatic {
                name: "value".to_string(),
                optional: false,
            },
            Instruction::Return,
        ]);

        assert!(matches!(
            optimized.as_slice(),
            [
                Instruction::LoadSlotGetPropStatic {
                    depth: 0,
                    slot: 1,
                    name,
                    optional: false,
                },
                Instruction::Return,
            ] if name == "value"
        ));
    }

    #[test]
    fn top_of_stack_peephole_rewrites_redundant_slot_loads_to_dup() {
        let optimized = Compiler::apply_top_of_stack_peephole(vec![
            Instruction::LoadSlot { depth: 0, slot: 1 },
            Instruction::LoadSlot { depth: 0, slot: 1 },
            Instruction::Return,
        ]);

        assert!(matches!(
            optimized.as_slice(),
            [
                Instruction::LoadSlot { depth: 0, slot: 1 },
                Instruction::Dup,
                Instruction::Return,
            ]
        ));
    }

    #[test]
    fn top_of_stack_peephole_rewrites_reload_after_store_to_dup() {
        let optimized = Compiler::apply_top_of_stack_peephole(vec![
            Instruction::PushNumber(1.0),
            Instruction::StoreSlot { depth: 0, slot: 1 },
            Instruction::LoadSlot { depth: 0, slot: 1 },
            Instruction::Return,
        ]);

        assert!(matches!(
            optimized.as_slice(),
            [
                Instruction::PushNumber(1.0),
                Instruction::StoreSlot { depth: 0, slot: 1 },
                Instruction::Dup,
                Instruction::Return,
            ]
        ));
    }

    #[test]
    fn top_of_stack_peephole_rewrites_redundant_literal_pushes_to_dup() {
        let optimized = Compiler::apply_top_of_stack_peephole(vec![
            Instruction::PushString("value".to_string()),
            Instruction::PushString("value".to_string()),
            Instruction::Return,
        ]);

        assert!(matches!(
            optimized.as_slice(),
            [
                Instruction::PushString(value),
                Instruction::Dup,
                Instruction::Return,
            ] if value == "value"
        ));
    }

    #[test]
    fn top_of_stack_peephole_flushes_across_call_boundaries() {
        let optimized = Compiler::apply_top_of_stack_peephole(vec![
            Instruction::LoadSlot { depth: 0, slot: 1 },
            Instruction::Call {
                argc: 0,
                with_this: false,
                optional: false,
            },
            Instruction::LoadSlot { depth: 0, slot: 1 },
            Instruction::Return,
        ]);

        assert!(matches!(
            optimized.as_slice(),
            [
                Instruction::LoadSlot { depth: 0, slot: 1 },
                Instruction::Call {
                    argc: 0,
                    with_this: false,
                    optional: false,
                },
                Instruction::LoadSlot { depth: 0, slot: 1 },
                Instruction::Return,
            ]
        ));
    }

    #[test]
    fn superinstruction_peephole_fuses_dup_get_prop_static_pairs() {
        let optimized = Compiler::apply_superinstruction_peephole(vec![
            Instruction::Dup,
            Instruction::GetPropStatic {
                name: "value".to_string(),
                optional: true,
            },
            Instruction::Return,
        ]);

        assert!(matches!(
            optimized.as_slice(),
            [
                Instruction::DupGetPropStatic {
                    name,
                    optional: true,
                },
                Instruction::Return,
            ] if name == "value"
        ));
    }

    #[test]
    fn superinstruction_peephole_fuses_load_slot_dup_get_prop_static_triplets() {
        let optimized = Compiler::apply_superinstruction_peephole(vec![
            Instruction::LoadSlot { depth: 0, slot: 1 },
            Instruction::Dup,
            Instruction::GetPropStatic {
                name: "value".to_string(),
                optional: false,
            },
            Instruction::Return,
        ]);

        assert!(matches!(
            optimized.as_slice(),
            [
                Instruction::LoadSlotDupGetPropStatic {
                    depth: 0,
                    slot: 1,
                    name,
                    optional: false,
                },
                Instruction::Return,
            ] if name == "value"
        ));
    }
}
