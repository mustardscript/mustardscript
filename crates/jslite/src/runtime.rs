use std::collections::{HashSet, VecDeque};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use slotmap::{SlotMap, new_key_type};

use crate::{
    diagnostic::{DiagnosticKind, JsliteError, JsliteResult, TraceFrame},
    ir::{
        AssignOp, AssignTarget, BinaryOp, BindingKind, CompiledProgram, Expr, ForInit,
        FunctionExpr, LogicalOp, MemberProperty, Pattern, PropertyName, Stmt, UnaryOp,
    },
    limits::RuntimeLimits,
    span::SourceSpan,
    structured::{StructuredNumber, StructuredValue},
};

new_key_type! { struct EnvKey; }
new_key_type! { struct CellKey; }
new_key_type! { struct ObjectKey; }
new_key_type! { struct ArrayKey; }
new_key_type! { struct ClosureKey; }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionOptions {
    pub inputs: IndexMap<String, StructuredValue>,
    pub capabilities: Vec<String>,
    pub limits: RuntimeLimits,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            inputs: IndexMap::new(),
            capabilities: Vec::new(),
            limits: RuntimeLimits::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostError {
    pub name: String,
    pub message: String,
    pub code: Option<String>,
    pub details: Option<StructuredValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResumePayload {
    Value(StructuredValue),
    Error(HostError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSnapshot {
    runtime: Runtime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suspension {
    pub capability: String,
    pub args: Vec<StructuredValue>,
    pub snapshot: ExecutionSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStep {
    Completed(StructuredValue),
    Suspended(Box<Suspension>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BytecodeProgram {
    pub functions: Vec<FunctionPrototype>,
    pub root: usize,
}

const SERIAL_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionPrototype {
    pub name: Option<String>,
    pub params: Vec<Pattern>,
    pub code: Vec<Instruction>,
    pub is_async: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Instruction {
    PushUndefined,
    PushNull,
    PushBool(bool),
    PushNumber(f64),
    PushString(String),
    LoadName(String),
    StoreName(String),
    InitializePattern(Pattern),
    PushEnv,
    PopEnv,
    DeclareName {
        name: String,
        mutable: bool,
    },
    MakeClosure {
        function_id: usize,
    },
    MakeArray {
        count: usize,
    },
    MakeObject {
        keys: Vec<PropertyName>,
    },
    GetPropStatic {
        name: String,
        optional: bool,
    },
    GetPropComputed {
        optional: bool,
    },
    SetPropStatic {
        name: String,
    },
    SetPropComputed,
    Unary(UnaryOp),
    Binary(BinaryOp),
    Pop,
    Dup,
    Dup2,
    Jump(usize),
    JumpIfFalse(usize),
    JumpIfTrue(usize),
    JumpIfNullish(usize),
    Call {
        argc: usize,
        with_this: bool,
        optional: bool,
    },
    Construct {
        argc: usize,
    },
    Return,
}

pub fn lower_to_bytecode(program: &CompiledProgram) -> JsliteResult<BytecodeProgram> {
    let mut compiler = Compiler::default();
    let root = compiler.compile_root(&program.script.body, program.script.span)?;
    let program = BytecodeProgram {
        functions: compiler.functions,
        root,
    };
    validate_bytecode_program(&program)?;
    Ok(program)
}

pub fn execute(
    program: &CompiledProgram,
    options: ExecutionOptions,
) -> JsliteResult<StructuredValue> {
    match start(program, options)? {
        ExecutionStep::Completed(value) => Ok(value),
        ExecutionStep::Suspended(suspension) => Err(JsliteError::runtime(format!(
            "execution suspended on capability `{}`; use start()/resume() for iterative execution",
            suspension.capability
        ))),
    }
}

pub fn start(program: &CompiledProgram, options: ExecutionOptions) -> JsliteResult<ExecutionStep> {
    let bytecode = lower_to_bytecode(program)?;
    start_bytecode(&bytecode, options)
}

pub fn start_bytecode(
    program: &BytecodeProgram,
    options: ExecutionOptions,
) -> JsliteResult<ExecutionStep> {
    validate_bytecode_program(program)?;
    let mut runtime = Runtime::new(program.clone(), options)?;
    runtime.run_root()
}

pub fn resume(snapshot: ExecutionSnapshot, payload: ResumePayload) -> JsliteResult<ExecutionStep> {
    let mut runtime = snapshot.runtime;
    runtime.resume(payload)
}

pub fn dump_program(program: &BytecodeProgram) -> JsliteResult<Vec<u8>> {
    bincode::serialize(&SerializedProgram {
        version: SERIAL_FORMAT_VERSION,
        program: program.clone(),
    })
    .map_err(|error| JsliteError::Message {
        kind: DiagnosticKind::Serialization,
        message: error.to_string(),
        span: None,
        traceback: Vec::new(),
    })
}

pub fn load_program(bytes: &[u8]) -> JsliteResult<BytecodeProgram> {
    let decoded: SerializedProgram =
        bincode::deserialize(bytes).map_err(|error| JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: error.to_string(),
            span: None,
            traceback: Vec::new(),
        })?;
    if decoded.version != SERIAL_FORMAT_VERSION {
        return Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: format!(
                "serialized program version mismatch: expected {}, got {}",
                SERIAL_FORMAT_VERSION, decoded.version
            ),
            span: None,
            traceback: Vec::new(),
        });
    }
    validate_bytecode_program(&decoded.program)?;
    Ok(decoded.program)
}

pub fn dump_snapshot(snapshot: &ExecutionSnapshot) -> JsliteResult<Vec<u8>> {
    bincode::serialize(&SerializedSnapshot {
        version: SERIAL_FORMAT_VERSION,
        snapshot: snapshot.clone(),
    })
    .map_err(|error| JsliteError::Message {
        kind: DiagnosticKind::Serialization,
        message: error.to_string(),
        span: None,
        traceback: Vec::new(),
    })
}

pub fn load_snapshot(bytes: &[u8]) -> JsliteResult<ExecutionSnapshot> {
    let decoded: SerializedSnapshot =
        bincode::deserialize(bytes).map_err(|error| JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: error.to_string(),
            span: None,
            traceback: Vec::new(),
        })?;
    if decoded.version != SERIAL_FORMAT_VERSION {
        return Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: format!(
                "serialized snapshot version mismatch: expected {}, got {}",
                SERIAL_FORMAT_VERSION, decoded.version
            ),
            span: None,
            traceback: Vec::new(),
        });
    }
    validate_snapshot(&decoded.snapshot)?;
    Ok(decoded.snapshot)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializedProgram {
    version: u32,
    program: BytecodeProgram,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializedSnapshot {
    version: u32,
    snapshot: ExecutionSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ValidationState {
    stack_depth: usize,
    scope_depth: usize,
}

fn validate_bytecode_program(program: &BytecodeProgram) -> JsliteResult<()> {
    if program.functions.is_empty() {
        return Err(JsliteError::validation(
            "bytecode validation failed: program defines no functions",
            None,
        ));
    }
    if program.root >= program.functions.len() {
        return Err(JsliteError::validation(
            format!(
                "bytecode validation failed: root function {} is out of range for {} functions",
                program.root,
                program.functions.len()
            ),
            None,
        ));
    }
    for (function_id, function) in program.functions.iter().enumerate() {
        validate_function(program, function_id, function)?;
    }
    Ok(())
}

fn validate_function(
    program: &BytecodeProgram,
    function_id: usize,
    function: &FunctionPrototype,
) -> JsliteResult<()> {
    if function.code.is_empty() {
        return Err(JsliteError::validation(
            format!("bytecode validation failed: function {function_id} has no instructions"),
            None,
        ));
    }
    if !matches!(function.code.last(), Some(Instruction::Return)) {
        return Err(JsliteError::validation(
            format!("bytecode validation failed: function {function_id} does not end in Return"),
            None,
        ));
    }

    let code_len = function.code.len();
    for (ip, instruction) in function.code.iter().enumerate() {
        match instruction {
            Instruction::MakeClosure {
                function_id: target,
            } if *target >= program.functions.len() => {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} references missing closure target {target}"
                    ),
                    None,
                ));
            }
            Instruction::Jump(target)
            | Instruction::JumpIfFalse(target)
            | Instruction::JumpIfTrue(target)
            | Instruction::JumpIfNullish(target)
                if *target >= code_len =>
            {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} jumps to invalid target {target}"
                    ),
                    None,
                ));
            }
            _ => {}
        }
    }

    let mut states = vec![None; code_len];
    let mut work = VecDeque::from([(
        0usize,
        ValidationState {
            stack_depth: 0,
            scope_depth: 0,
        },
    )]);
    while let Some((ip, state)) = work.pop_front() {
        if let Some(existing) = states[ip] {
            if existing != state {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} has inconsistent stack or scope depth"
                    ),
                    None,
                ));
            }
            continue;
        }
        states[ip] = Some(state);

        let instruction = &function.code[ip];
        let next_state = apply_validation_effect(function_id, ip, instruction, state)?;
        for successor in validation_successors(ip, instruction, code_len) {
            work.push_back((successor, next_state));
        }
    }

    Ok(())
}

fn apply_validation_effect(
    function_id: usize,
    ip: usize,
    instruction: &Instruction,
    state: ValidationState,
) -> JsliteResult<ValidationState> {
    let require_stack = |count: usize| -> JsliteResult<()> {
        if state.stack_depth < count {
            return Err(JsliteError::validation(
                format!(
                    "bytecode validation failed: function {function_id} instruction {ip} requires stack depth {count}, found {}",
                    state.stack_depth
                ),
                None,
            ));
        }
        Ok(())
    };

    let next = match instruction {
        Instruction::PushUndefined
        | Instruction::PushNull
        | Instruction::PushBool(_)
        | Instruction::PushNumber(_)
        | Instruction::PushString(_)
        | Instruction::LoadName(_)
        | Instruction::MakeClosure { .. } => ValidationState {
            stack_depth: state.stack_depth + 1,
            ..state
        },
        Instruction::StoreName(_) => {
            require_stack(1)?;
            state
        }
        Instruction::InitializePattern(_) | Instruction::Pop => {
            require_stack(1)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
                ..state
            }
        }
        Instruction::PushEnv => ValidationState {
            scope_depth: state.scope_depth + 1,
            ..state
        },
        Instruction::PopEnv => {
            if state.scope_depth == 0 {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} pops an empty scope stack"
                    ),
                    None,
                ));
            }
            ValidationState {
                scope_depth: state.scope_depth - 1,
                ..state
            }
        }
        Instruction::DeclareName { .. } => state,
        Instruction::MakeArray { count } => {
            require_stack(*count)?;
            ValidationState {
                stack_depth: state.stack_depth - count + 1,
                ..state
            }
        }
        Instruction::MakeObject { keys } => {
            require_stack(keys.len())?;
            ValidationState {
                stack_depth: state.stack_depth - keys.len() + 1,
                ..state
            }
        }
        Instruction::GetPropStatic { .. } => {
            require_stack(1)?;
            state
        }
        Instruction::GetPropComputed { .. } => {
            require_stack(2)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
                ..state
            }
        }
        Instruction::SetPropStatic { .. } => {
            require_stack(2)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
                ..state
            }
        }
        Instruction::SetPropComputed => {
            require_stack(3)?;
            ValidationState {
                stack_depth: state.stack_depth - 2,
                ..state
            }
        }
        Instruction::Unary(_) => {
            require_stack(1)?;
            state
        }
        Instruction::Binary(_) => {
            require_stack(2)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
                ..state
            }
        }
        Instruction::Dup => {
            require_stack(1)?;
            ValidationState {
                stack_depth: state.stack_depth + 1,
                ..state
            }
        }
        Instruction::Dup2 => {
            require_stack(2)?;
            ValidationState {
                stack_depth: state.stack_depth + 2,
                ..state
            }
        }
        Instruction::Jump(_) => state,
        Instruction::JumpIfFalse(_)
        | Instruction::JumpIfTrue(_)
        | Instruction::JumpIfNullish(_) => {
            require_stack(1)?;
            state
        }
        Instruction::Call {
            argc, with_this, ..
        } => {
            let required = argc + 1 + usize::from(*with_this);
            require_stack(required)?;
            ValidationState {
                stack_depth: state.stack_depth - required + 1,
                ..state
            }
        }
        Instruction::Construct { argc } => {
            let required = argc + 1;
            require_stack(required)?;
            ValidationState {
                stack_depth: state.stack_depth - required + 1,
                ..state
            }
        }
        Instruction::Return => state,
    };
    Ok(next)
}

fn validation_successors(ip: usize, instruction: &Instruction, code_len: usize) -> Vec<usize> {
    match instruction {
        Instruction::Jump(target) => vec![*target],
        Instruction::JumpIfFalse(target)
        | Instruction::JumpIfTrue(target)
        | Instruction::JumpIfNullish(target) => {
            let mut successors = vec![*target];
            if ip + 1 < code_len {
                successors.push(ip + 1);
            }
            successors
        }
        Instruction::Return => Vec::new(),
        _ if ip + 1 < code_len => vec![ip + 1],
        _ => Vec::new(),
    }
}

fn validate_snapshot(snapshot: &ExecutionSnapshot) -> JsliteResult<()> {
    let runtime = &snapshot.runtime;
    validate_bytecode_program(&runtime.program)?;
    if runtime.envs.get(runtime.globals).is_none() {
        return Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: "snapshot validation failed: missing globals environment".to_string(),
            span: None,
            traceback: Vec::new(),
        });
    }
    if runtime.frames.is_empty() {
        return Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: "snapshot validation failed: suspended runtime has no frames".to_string(),
            span: None,
            traceback: Vec::new(),
        });
    }

    for (env_key, env) in &runtime.envs {
        if let Some(parent) = env.parent
            && runtime.envs.get(parent).is_none()
        {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: environment {:?} references missing parent {:?}",
                    env_key, parent
                ),
                span: None,
                traceback: Vec::new(),
            });
        }
        for cell in env.bindings.values() {
            if runtime.cells.get(*cell).is_none() {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message: format!(
                        "snapshot validation failed: environment {:?} references missing cell {:?}",
                        env_key, cell
                    ),
                    span: None,
                    traceback: Vec::new(),
                });
            }
        }
    }

    for (closure_key, closure) in &runtime.closures {
        if closure.function_id >= runtime.program.functions.len() {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: closure {:?} references missing function {}",
                    closure_key, closure.function_id
                ),
                span: None,
                traceback: Vec::new(),
            });
        }
        if runtime.envs.get(closure.env).is_none() {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: closure {:?} references missing environment {:?}",
                    closure_key, closure.env
                ),
                span: None,
                traceback: Vec::new(),
            });
        }
    }

    for frame in &runtime.frames {
        let Some(function) = runtime.program.functions.get(frame.function_id) else {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: frame references missing function {}",
                    frame.function_id
                ),
                span: None,
                traceback: Vec::new(),
            });
        };
        if frame.ip >= function.code.len() {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: frame instruction pointer {} is out of range for function {}",
                    frame.ip, frame.function_id
                ),
                span: None,
                traceback: Vec::new(),
            });
        }
        if runtime.envs.get(frame.env).is_none() {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: frame references missing environment {:?}",
                    frame.env
                ),
                span: None,
                traceback: Vec::new(),
            });
        }
        for env in &frame.scope_stack {
            if runtime.envs.get(*env).is_none() {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message: format!(
                        "snapshot validation failed: frame scope stack references missing environment {:?}",
                        env
                    ),
                    span: None,
                    traceback: Vec::new(),
                });
            }
        }
        for value in &frame.stack {
            validate_runtime_value(runtime, value)?;
        }
    }

    for cell in runtime.cells.values() {
        validate_runtime_value(runtime, &cell.value)?;
    }
    for object in runtime.objects.values() {
        for value in object.properties.values() {
            validate_runtime_value(runtime, value)?;
        }
    }
    for array in runtime.arrays.values() {
        for value in &array.elements {
            validate_runtime_value(runtime, value)?;
        }
        for value in array.properties.values() {
            validate_runtime_value(runtime, value)?;
        }
    }

    Ok(())
}

fn validate_runtime_value(runtime: &Runtime, value: &Value) -> JsliteResult<()> {
    match value {
        Value::Object(object) if runtime.objects.get(*object).is_none() => {
            Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: value references missing object {:?}",
                    object
                ),
                span: None,
                traceback: Vec::new(),
            })
        }
        Value::Array(array) if runtime.arrays.get(*array).is_none() => Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: format!(
                "snapshot validation failed: value references missing array {:?}",
                array
            ),
            span: None,
            traceback: Vec::new(),
        }),
        Value::Closure(closure) if runtime.closures.get(*closure).is_none() => {
            Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: value references missing closure {:?}",
                    closure
                ),
                span: None,
                traceback: Vec::new(),
            })
        }
        _ => Ok(()),
    }
}

#[derive(Debug, Default)]
struct Compiler {
    functions: Vec<FunctionPrototype>,
}

#[derive(Debug, Default)]
struct CompileContext {
    code: Vec<Instruction>,
    loop_stack: Vec<LoopContext>,
    scope_depth: usize,
}

#[derive(Debug, Default)]
struct LoopContext {
    break_jumps: Vec<usize>,
    continue_jumps: Vec<usize>,
    continue_target: Option<usize>,
    scope_depth: usize,
}

impl Compiler {
    fn compile_root(&mut self, statements: &[Stmt], span: SourceSpan) -> JsliteResult<usize> {
        let mut context = CompileContext::default();
        self.emit_block_prologue(&mut context, statements)?;
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
            params: Vec::new(),
            code: context.code,
            is_async: false,
            span,
        });
        Ok(id)
    }

    fn compile_function(&mut self, function: &FunctionExpr) -> JsliteResult<usize> {
        self.compile_function_body(
            function.name.clone(),
            &function.params,
            &function.body,
            function.is_async,
            function.span,
        )
    }

    fn compile_function_body(
        &mut self,
        name: Option<String>,
        params: &[Pattern],
        statements: &[Stmt],
        is_async: bool,
        span: SourceSpan,
    ) -> JsliteResult<usize> {
        let mut context = CompileContext::default();
        self.emit_block_prologue(&mut context, statements)?;
        for statement in statements {
            self.compile_stmt(&mut context, statement)?;
        }
        context.code.push(Instruction::PushUndefined);
        context.code.push(Instruction::Return);
        let id = self.functions.len();
        self.functions.push(FunctionPrototype {
            name,
            params: params.to_vec(),
            code: context.code,
            is_async,
            span,
        });
        Ok(id)
    }

    fn emit_block_prologue(
        &mut self,
        context: &mut CompileContext,
        statements: &[Stmt],
    ) -> JsliteResult<()> {
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
            if declared.insert(function.name.clone()) {
                context.code.push(Instruction::DeclareName {
                    name: function.name.clone(),
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
                    name: function.name,
                }));
        }
        Ok(())
    }

    fn compile_stmt(&mut self, context: &mut CompileContext, statement: &Stmt) -> JsliteResult<()> {
        match statement {
            Stmt::Block { body, .. } => {
                context.code.push(Instruction::PushEnv);
                context.scope_depth += 1;
                self.emit_block_prologue(context, body)?;
                for statement in body {
                    self.compile_stmt(context, statement)?;
                }
                context.scope_depth -= 1;
                context.code.push(Instruction::PopEnv);
            }
            Stmt::VariableDecl { declarators, .. } => {
                for declarator in declarators {
                    if let Some(initializer) = &declarator.initializer {
                        self.compile_expr(context, initializer)?;
                    } else {
                        context.code.push(Instruction::PushUndefined);
                    }
                    context
                        .code
                        .push(Instruction::InitializePattern(declarator.pattern.clone()));
                }
            }
            Stmt::FunctionDecl { .. } => {}
            Stmt::Expression { expression, .. } => {
                self.compile_expr(context, expression)?;
                context.code.push(Instruction::Pop);
            }
            Stmt::If {
                test,
                consequent,
                alternate,
                ..
            } => {
                self.compile_expr(context, test)?;
                let jump_to_else = self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                context.code.push(Instruction::Pop);
                self.compile_stmt(context, consequent)?;
                let jump_to_end = self.emit_jump(context, Instruction::Jump(usize::MAX));
                let else_ip = context.code.len();
                self.patch_jump(context, jump_to_else, else_ip);
                context.code.push(Instruction::Pop);
                if let Some(alternate) = alternate {
                    self.compile_stmt(context, alternate)?;
                }
                let end_ip = context.code.len();
                self.patch_jump(context, jump_to_end, end_ip);
            }
            Stmt::While { test, body, .. } => {
                let loop_start = context.code.len();
                self.compile_expr(context, test)?;
                let exit_jump = self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                context.code.push(Instruction::Pop);
                context.loop_stack.push(LoopContext {
                    scope_depth: context.scope_depth,
                    ..LoopContext::default()
                });
                self.compile_stmt(context, body)?;
                let loop_ctx = context.loop_stack.pop().unwrap_or_default();
                let continue_target = loop_ctx.continue_target.unwrap_or(loop_start);
                for jump in loop_ctx.continue_jumps {
                    self.patch_jump(context, jump, continue_target);
                }
                context.code.push(Instruction::Jump(loop_start));
                let false_path_ip = context.code.len();
                context.code.push(Instruction::Pop);
                let loop_end = context.code.len();
                self.patch_jump(context, exit_jump, false_path_ip);
                for jump in loop_ctx.break_jumps {
                    self.patch_jump(context, jump, loop_end);
                }
            }
            Stmt::DoWhile { body, test, .. } => {
                let loop_start = context.code.len();
                context.loop_stack.push(LoopContext {
                    scope_depth: context.scope_depth,
                    ..LoopContext::default()
                });
                self.compile_stmt(context, body)?;
                let continue_target = context.code.len();
                if let Some(loop_ctx) = context.loop_stack.last_mut() {
                    loop_ctx.continue_target = Some(continue_target);
                }
                self.compile_expr(context, test)?;
                let exit_jump = self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                context.code.push(Instruction::Pop);
                context.code.push(Instruction::Jump(loop_start));
                let false_path_ip = context.code.len();
                context.code.push(Instruction::Pop);
                let loop_end = context.code.len();
                self.patch_jump(context, exit_jump, false_path_ip);
                let loop_ctx = context.loop_stack.pop().unwrap_or_default();
                for jump in loop_ctx.continue_jumps {
                    self.patch_jump(context, jump, continue_target);
                }
                for jump in loop_ctx.break_jumps {
                    self.patch_jump(context, jump, loop_end);
                }
            }
            Stmt::For {
                init,
                test,
                update,
                body,
                ..
            } => {
                context.code.push(Instruction::PushEnv);
                context.scope_depth += 1;
                if let Some(init) = init {
                    match init {
                        ForInit::VariableDecl {
                            kind: _,
                            declarators,
                        } => {
                            for declarator in declarators {
                                for (name, mutable) in pattern_bindings(&declarator.pattern) {
                                    context
                                        .code
                                        .push(Instruction::DeclareName { name, mutable });
                                }
                                if let Some(initializer) = &declarator.initializer {
                                    self.compile_expr(context, initializer)?;
                                } else {
                                    context.code.push(Instruction::PushUndefined);
                                }
                                context.code.push(Instruction::InitializePattern(
                                    declarator.pattern.clone(),
                                ));
                            }
                        }
                        ForInit::Expression(expression) => {
                            self.compile_expr(context, expression)?;
                            context.code.push(Instruction::Pop);
                        }
                    }
                }
                let loop_start = context.code.len();
                let exit_jump = if let Some(test) = test {
                    self.compile_expr(context, test)?;
                    let jump = self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                    context.code.push(Instruction::Pop);
                    Some(jump)
                } else {
                    None
                };
                context.loop_stack.push(LoopContext {
                    scope_depth: context.scope_depth,
                    ..LoopContext::default()
                });
                self.compile_stmt(context, body)?;
                let update_start = context.code.len();
                if let Some(loop_ctx) = context.loop_stack.last_mut() {
                    loop_ctx.continue_target = Some(update_start);
                }
                if let Some(update) = update {
                    self.compile_expr(context, update)?;
                    context.code.push(Instruction::Pop);
                }
                context.code.push(Instruction::Jump(loop_start));
                let false_path_ip = context.code.len();
                if exit_jump.is_some() {
                    context.code.push(Instruction::Pop);
                }
                let loop_end = context.code.len();
                if let Some(exit_jump) = exit_jump {
                    self.patch_jump(context, exit_jump, false_path_ip);
                }
                let loop_ctx = context.loop_stack.pop().unwrap_or_default();
                for jump in loop_ctx.continue_jumps {
                    self.patch_jump(context, jump, update_start);
                }
                for jump in loop_ctx.break_jumps {
                    self.patch_jump(context, jump, loop_end);
                }
                context.scope_depth -= 1;
                context.code.push(Instruction::PopEnv);
            }
            Stmt::Break { span } => {
                if let Some(loop_ctx) = context.loop_stack.last() {
                    for _ in loop_ctx.scope_depth..context.scope_depth {
                        context.code.push(Instruction::PopEnv);
                    }
                }
                let jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
                if let Some(loop_ctx) = context.loop_stack.last_mut() {
                    loop_ctx.break_jumps.push(jump);
                } else {
                    return Err(JsliteError::runtime_at(
                        "`break` used outside of a loop",
                        *span,
                    ));
                }
            }
            Stmt::Continue { span } => {
                if let Some(loop_ctx) = context.loop_stack.last() {
                    for _ in loop_ctx.scope_depth..context.scope_depth {
                        context.code.push(Instruction::PopEnv);
                    }
                }
                let jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
                if let Some(loop_ctx) = context.loop_stack.last_mut() {
                    loop_ctx.continue_jumps.push(jump);
                } else {
                    return Err(JsliteError::runtime_at(
                        "`continue` used outside of a loop",
                        *span,
                    ));
                }
            }
            Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    self.compile_expr(context, value)?;
                } else {
                    context.code.push(Instruction::PushUndefined);
                }
                context.code.push(Instruction::Return);
            }
            Stmt::Throw { span, .. } => {
                return Err(JsliteError::runtime_at(
                    "runtime support for throw/try/catch/finally is not implemented yet",
                    *span,
                ));
            }
            Stmt::Try { span, .. } => {
                return Err(JsliteError::runtime_at(
                    "runtime support for try/catch/finally is not implemented yet",
                    *span,
                ));
            }
            Stmt::Switch {
                discriminant,
                cases,
                ..
            } => {
                self.compile_expr(context, discriminant)?;
                let mut case_jumps = Vec::new();
                let mut default_case_index = None;
                context.loop_stack.push(LoopContext {
                    scope_depth: context.scope_depth,
                    ..LoopContext::default()
                });
                for (case_index, case) in cases.iter().enumerate() {
                    if let Some(test) = &case.test {
                        context.code.push(Instruction::Dup);
                        self.compile_expr(context, test)?;
                        context.code.push(Instruction::Binary(BinaryOp::StrictEq));
                        let miss_jump =
                            self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                        context.code.push(Instruction::Pop);
                        context.code.push(Instruction::Pop);
                        case_jumps.push(self.emit_jump(context, Instruction::Jump(usize::MAX)));
                        let miss_ip = context.code.len();
                        self.patch_jump(context, miss_jump, miss_ip);
                        context.code.push(Instruction::Pop);
                    } else {
                        default_case_index = Some(case_index);
                    }
                }
                context.code.push(Instruction::Pop);
                let jump_past_cases = self.emit_jump(context, Instruction::Jump(usize::MAX));
                let mut case_offsets = Vec::new();
                for case in cases {
                    let case_start = context.code.len();
                    case_offsets.push(case_start);
                    for statement in &case.consequent {
                        self.compile_stmt(context, statement)?;
                    }
                }
                let end_ip = context.code.len();
                let default_target = default_case_index
                    .and_then(|index| case_offsets.get(index).copied())
                    .unwrap_or(end_ip);
                self.patch_jump(context, jump_past_cases, default_target);
                for (jump, target) in case_jumps.into_iter().zip(case_offsets.iter().copied()) {
                    self.patch_jump(context, jump, target);
                }
                let loop_ctx = context.loop_stack.pop().unwrap_or_default();
                for jump in loop_ctx.break_jumps {
                    self.patch_jump(context, jump, end_ip);
                }
            }
            Stmt::Empty { .. } => {}
        }
        Ok(())
    }

    fn compile_expr(
        &mut self,
        context: &mut CompileContext,
        expression: &Expr,
    ) -> JsliteResult<()> {
        match expression {
            Expr::Undefined { .. } => context.code.push(Instruction::PushUndefined),
            Expr::Null { .. } => context.code.push(Instruction::PushNull),
            Expr::Bool { value, .. } => context.code.push(Instruction::PushBool(*value)),
            Expr::Number { value, .. } => context.code.push(Instruction::PushNumber(*value)),
            Expr::String { value, .. } => context.code.push(Instruction::PushString(value.clone())),
            Expr::Identifier { name, .. } => context.code.push(Instruction::LoadName(name.clone())),
            Expr::This { .. } => context.code.push(Instruction::LoadName("this".to_string())),
            Expr::Array { elements, .. } => {
                for element in elements {
                    self.compile_expr(context, element)?;
                }
                context.code.push(Instruction::MakeArray {
                    count: elements.len(),
                });
            }
            Expr::Object { properties, .. } => {
                let mut keys = Vec::with_capacity(properties.len());
                for property in properties {
                    self.compile_expr(context, &property.value)?;
                    keys.push(property.key.clone());
                }
                context.code.push(Instruction::MakeObject { keys });
            }
            Expr::Function(function) => {
                context.code.push(Instruction::MakeClosure {
                    function_id: self.compile_function(function)?,
                });
            }
            Expr::Unary {
                operator, argument, ..
            } => {
                self.compile_expr(context, argument)?;
                context.code.push(Instruction::Unary(*operator));
            }
            Expr::Binary {
                operator,
                left,
                right,
                ..
            } => {
                self.compile_expr(context, left)?;
                self.compile_expr(context, right)?;
                context.code.push(Instruction::Binary(*operator));
            }
            Expr::Logical {
                operator,
                left,
                right,
                ..
            } => {
                self.compile_expr(context, left)?;
                match operator {
                    LogicalOp::And => {
                        let jump = self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                        context.code.push(Instruction::Pop);
                        self.compile_expr(context, right)?;
                        let end_ip = context.code.len();
                        self.patch_jump(context, jump, end_ip);
                    }
                    LogicalOp::Or => {
                        let jump = self.emit_jump(context, Instruction::JumpIfTrue(usize::MAX));
                        context.code.push(Instruction::Pop);
                        self.compile_expr(context, right)?;
                        let end_ip = context.code.len();
                        self.patch_jump(context, jump, end_ip);
                    }
                    LogicalOp::NullishCoalesce => {
                        let jump = self.emit_jump(context, Instruction::JumpIfNullish(usize::MAX));
                        let end = self.emit_jump(context, Instruction::Jump(usize::MAX));
                        let rhs = context.code.len();
                        self.patch_jump(context, jump, rhs);
                        context.code.push(Instruction::Pop);
                        self.compile_expr(context, right)?;
                        let end_ip = context.code.len();
                        self.patch_jump(context, end, end_ip);
                    }
                }
            }
            Expr::Conditional {
                test,
                consequent,
                alternate,
                ..
            } => {
                self.compile_expr(context, test)?;
                let else_jump = self.emit_jump(context, Instruction::JumpIfFalse(usize::MAX));
                self.compile_expr(context, consequent)?;
                let end_jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
                let else_ip = context.code.len();
                self.patch_jump(context, else_jump, else_ip);
                self.compile_expr(context, alternate)?;
                let end_ip = context.code.len();
                self.patch_jump(context, end_jump, end_ip);
            }
            Expr::Assignment {
                target,
                operator,
                value,
                ..
            } => self.compile_assignment(context, target, *operator, value)?,
            Expr::Member {
                object,
                property,
                optional,
                ..
            } => {
                self.compile_expr(context, object)?;
                match property {
                    MemberProperty::Static(PropertyName::Identifier(name))
                    | MemberProperty::Static(PropertyName::String(name)) => {
                        context.code.push(Instruction::GetPropStatic {
                            name: name.clone(),
                            optional: *optional,
                        })
                    }
                    MemberProperty::Static(PropertyName::Number(number)) => {
                        context.code.push(Instruction::GetPropStatic {
                            name: format_number_key(*number),
                            optional: *optional,
                        })
                    }
                    MemberProperty::Computed(expression) => {
                        self.compile_expr(context, expression)?;
                        context.code.push(Instruction::GetPropComputed {
                            optional: *optional,
                        });
                    }
                }
            }
            Expr::Call {
                callee,
                arguments,
                optional,
                ..
            } => {
                let with_this = matches!(callee.as_ref(), Expr::Member { .. });
                if let Expr::Member {
                    object,
                    property,
                    optional: member_optional,
                    ..
                } = callee.as_ref()
                {
                    self.compile_expr(context, object)?;
                    context.code.push(Instruction::Dup);
                    match property {
                        MemberProperty::Static(PropertyName::Identifier(name))
                        | MemberProperty::Static(PropertyName::String(name)) => {
                            context.code.push(Instruction::GetPropStatic {
                                name: name.clone(),
                                optional: *member_optional,
                            })
                        }
                        MemberProperty::Static(PropertyName::Number(number)) => {
                            context.code.push(Instruction::GetPropStatic {
                                name: format_number_key(*number),
                                optional: *member_optional,
                            })
                        }
                        MemberProperty::Computed(expression) => {
                            self.compile_expr(context, expression)?;
                            context.code.push(Instruction::GetPropComputed {
                                optional: *member_optional,
                            });
                        }
                    }
                } else {
                    self.compile_expr(context, callee)?;
                }
                for argument in arguments {
                    self.compile_expr(context, argument)?;
                }
                context.code.push(Instruction::Call {
                    argc: arguments.len(),
                    with_this,
                    optional: *optional,
                });
            }
            Expr::New {
                callee, arguments, ..
            } => {
                self.compile_expr(context, callee)?;
                for argument in arguments {
                    self.compile_expr(context, argument)?;
                }
                context.code.push(Instruction::Construct {
                    argc: arguments.len(),
                });
            }
            Expr::Template {
                quasis,
                expressions,
                ..
            } => {
                let mut parts = Vec::new();
                for (index, quasi) in quasis.iter().enumerate() {
                    if !quasi.is_empty() {
                        parts.push(Expr::String {
                            span: SourceSpan::new(0, 0),
                            value: quasi.clone(),
                        });
                    }
                    if let Some(expression) = expressions.get(index) {
                        parts.push(expression.clone());
                    }
                }
                if parts.is_empty() {
                    context.code.push(Instruction::PushString(String::new()));
                } else {
                    self.compile_expr(context, &parts[0])?;
                    for part in parts.iter().skip(1) {
                        self.compile_expr(context, part)?;
                        context.code.push(Instruction::Binary(BinaryOp::Add));
                    }
                }
            }
            Expr::Await { span, .. } => {
                return Err(JsliteError::runtime_at(
                    "runtime support for async/await is not implemented yet",
                    *span,
                ));
            }
        }
        Ok(())
    }

    fn compile_assignment(
        &mut self,
        context: &mut CompileContext,
        target: &AssignTarget,
        operator: AssignOp,
        value: &Expr,
    ) -> JsliteResult<()> {
        match target {
            AssignTarget::Identifier { name, .. } => {
                if operator == AssignOp::Assign {
                    self.compile_expr(context, value)?;
                    context.code.push(Instruction::StoreName(name.clone()));
                } else {
                    context.code.push(Instruction::LoadName(name.clone()));
                    self.compile_expr(context, value)?;
                    context
                        .code
                        .push(Instruction::Binary(assign_op_to_binary(operator)?));
                    context.code.push(Instruction::StoreName(name.clone()));
                }
            }
            AssignTarget::Member {
                object,
                property,
                optional,
                ..
            } => match property {
                MemberProperty::Static(PropertyName::Identifier(name))
                | MemberProperty::Static(PropertyName::String(name)) => {
                    self.compile_expr(context, object)?;
                    if operator == AssignOp::Assign {
                        self.compile_expr(context, value)?;
                        context
                            .code
                            .push(Instruction::SetPropStatic { name: name.clone() });
                    } else {
                        context.code.push(Instruction::Dup);
                        context.code.push(Instruction::GetPropStatic {
                            name: name.clone(),
                            optional: *optional,
                        });
                        self.compile_expr(context, value)?;
                        context
                            .code
                            .push(Instruction::Binary(assign_op_to_binary(operator)?));
                        context
                            .code
                            .push(Instruction::SetPropStatic { name: name.clone() });
                    }
                }
                MemberProperty::Static(PropertyName::Number(number)) => {
                    self.compile_expr(context, object)?;
                    let name = format_number_key(*number);
                    if operator == AssignOp::Assign {
                        self.compile_expr(context, value)?;
                        context.code.push(Instruction::SetPropStatic { name });
                    } else {
                        context.code.push(Instruction::Dup);
                        context.code.push(Instruction::GetPropStatic {
                            name: name.clone(),
                            optional: *optional,
                        });
                        self.compile_expr(context, value)?;
                        context
                            .code
                            .push(Instruction::Binary(assign_op_to_binary(operator)?));
                        context.code.push(Instruction::SetPropStatic { name });
                    }
                }
                MemberProperty::Computed(expr) => {
                    self.compile_expr(context, object)?;
                    self.compile_expr(context, expr)?;
                    if operator == AssignOp::Assign {
                        self.compile_expr(context, value)?;
                        context.code.push(Instruction::SetPropComputed);
                    } else {
                        context.code.push(Instruction::Dup2);
                        context.code.push(Instruction::GetPropComputed {
                            optional: *optional,
                        });
                        self.compile_expr(context, value)?;
                        context
                            .code
                            .push(Instruction::Binary(assign_op_to_binary(operator)?));
                        context.code.push(Instruction::SetPropComputed);
                    }
                }
            },
        }
        Ok(())
    }

    fn emit_jump(&self, context: &mut CompileContext, instruction: Instruction) -> usize {
        let index = context.code.len();
        context.code.push(instruction);
        index
    }

    fn patch_jump(&self, context: &mut CompileContext, index: usize, target: usize) {
        match &mut context.code[index] {
            Instruction::Jump(address)
            | Instruction::JumpIfFalse(address)
            | Instruction::JumpIfTrue(address)
            | Instruction::JumpIfNullish(address) => *address = target,
            _ => {}
        }
    }
}

#[derive(Debug)]
struct BlockBindings {
    lexicals: Vec<(String, bool)>,
    functions: Vec<FunctionBinding>,
}

#[derive(Debug)]
struct FunctionBinding {
    name: String,
    expr: FunctionExpr,
}

fn collect_block_bindings(statements: &[Stmt]) -> BlockBindings {
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

fn pattern_bindings(pattern: &Pattern) -> Vec<(String, bool)> {
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

fn assign_op_to_binary(operator: AssignOp) -> JsliteResult<BinaryOp> {
    match operator {
        AssignOp::Assign => Err(JsliteError::runtime("invalid compound assignment")),
        AssignOp::AddAssign => Ok(BinaryOp::Add),
        AssignOp::SubAssign => Ok(BinaryOp::Sub),
        AssignOp::MulAssign => Ok(BinaryOp::Mul),
        AssignOp::DivAssign => Ok(BinaryOp::Div),
        AssignOp::NullishAssign => Ok(BinaryOp::Add),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Value {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Object(ObjectKey),
    Array(ArrayKey),
    Closure(ClosureKey),
    BuiltinFunction(BuiltinFunction),
    HostFunction(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum BuiltinFunction {
    ArrayCtor,
    ArrayIsArray,
    ObjectCtor,
    NumberCtor,
    StringCtor,
    BooleanCtor,
    MathAbs,
    MathMax,
    MathMin,
    MathFloor,
    MathCeil,
    MathRound,
    JsonStringify,
    JsonParse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Env {
    parent: Option<EnvKey>,
    bindings: IndexMap<String, CellKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Cell {
    value: Value,
    mutable: bool,
    initialized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlainObject {
    properties: IndexMap<String, Value>,
    kind: ObjectKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ObjectKind {
    Plain,
    Global,
    Math,
    Json,
    Console,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArrayObject {
    elements: Vec<Value>,
    properties: IndexMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Closure {
    function_id: usize,
    env: EnvKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Frame {
    function_id: usize,
    ip: usize,
    env: EnvKey,
    scope_stack: Vec<EnvKey>,
    stack: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Runtime {
    program: BytecodeProgram,
    limits: RuntimeLimits,
    globals: EnvKey,
    envs: SlotMap<EnvKey, Env>,
    cells: SlotMap<CellKey, Cell>,
    objects: SlotMap<ObjectKey, PlainObject>,
    arrays: SlotMap<ArrayKey, ArrayObject>,
    closures: SlotMap<ClosureKey, Closure>,
    frames: Vec<Frame>,
    instruction_counter: usize,
}

enum RunState {
    Completed(Value),
    PushedFrame,
    Suspended {
        capability: String,
        args: Vec<StructuredValue>,
    },
}

enum StepAction {
    Continue,
    Return(ExecutionStep),
}

impl Runtime {
    fn new(program: BytecodeProgram, options: ExecutionOptions) -> JsliteResult<Self> {
        let mut envs = SlotMap::with_key();
        let globals = envs.insert(Env {
            parent: None,
            bindings: IndexMap::new(),
        });
        let mut runtime = Self {
            program,
            limits: options.limits,
            globals,
            envs,
            cells: SlotMap::with_key(),
            objects: SlotMap::with_key(),
            arrays: SlotMap::with_key(),
            closures: SlotMap::with_key(),
            frames: Vec::new(),
            instruction_counter: 0,
        };
        runtime.install_builtins()?;
        for capability in options.capabilities {
            runtime.define_global(capability.clone(), Value::HostFunction(capability), false)?;
        }
        for (name, value) in options.inputs {
            let value = runtime.value_from_structured(value)?;
            runtime.define_global(name, value, true)?;
        }
        Ok(runtime)
    }

    fn run_root(&mut self) -> JsliteResult<ExecutionStep> {
        let root_env = self.new_env(Some(self.globals));
        self.push_frame(self.program.root, root_env, &[])?;
        self.run()
    }

    fn traceback_frames(&self) -> Vec<TraceFrame> {
        self.frames
            .iter()
            .rev()
            .filter_map(|frame| {
                self.program
                    .functions
                    .get(frame.function_id)
                    .map(|function| TraceFrame {
                        function_name: function.name.clone(),
                        span: function.span,
                    })
            })
            .collect()
    }

    fn annotate_runtime_error(&self, error: JsliteError) -> JsliteError {
        error.with_traceback(self.traceback_frames())
    }

    fn resume(&mut self, payload: ResumePayload) -> JsliteResult<ExecutionStep> {
        match payload {
            ResumePayload::Value(value) => {
                let value = self
                    .value_from_structured(value)
                    .map_err(|error| self.annotate_runtime_error(error))?;
                let Some(frame) = self.frames.last_mut() else {
                    return Err(self.annotate_runtime_error(JsliteError::runtime(
                        "no suspended frame available",
                    )));
                };
                frame.stack.push(value);
            }
            ResumePayload::Error(error) => {
                return Err(self.annotate_runtime_error(JsliteError::runtime(format!(
                    "{}: {}{}{}",
                    error.name,
                    error.message,
                    error
                        .code
                        .as_ref()
                        .map(|code| format!(" [code={code}]"))
                        .unwrap_or_default(),
                    error
                        .details
                        .as_ref()
                        .map(|details| format!(" [details={details:?}]"))
                        .unwrap_or_default()
                ))));
            }
        }
        self.run()
    }

    fn run(&mut self) -> JsliteResult<ExecutionStep> {
        loop {
            let frame_index = self
                .frames
                .len()
                .checked_sub(1)
                .ok_or_else(|| JsliteError::runtime("vm lost all frames"))?;
            let function_id = self.frames[frame_index].function_id;
            let ip = self.frames[frame_index].ip;
            let instruction = self
                .program
                .functions
                .get(function_id)
                .and_then(|function| function.code.get(ip))
                .cloned()
                .ok_or_else(|| JsliteError::runtime("instruction pointer out of range"))
                .map_err(|error| self.annotate_runtime_error(error))?;
            self.frames[frame_index].ip += 1;
            self.bump_instruction_budget()
                .map_err(|error| self.annotate_runtime_error(error))?;
            let action = (|| -> JsliteResult<StepAction> {
                match instruction {
                    Instruction::PushUndefined => {
                        self.frames[frame_index].stack.push(Value::Undefined);
                    }
                    Instruction::PushNull => self.frames[frame_index].stack.push(Value::Null),
                    Instruction::PushBool(value) => {
                        self.frames[frame_index].stack.push(Value::Bool(value))
                    }
                    Instruction::PushNumber(value) => {
                        self.frames[frame_index].stack.push(Value::Number(value))
                    }
                    Instruction::PushString(value) => {
                        self.frames[frame_index].stack.push(Value::String(value))
                    }
                    Instruction::LoadName(name) => {
                        let env = self.frames[frame_index].env;
                        let value = self.lookup_name(env, &name)?;
                        self.frames[frame_index].stack.push(value);
                    }
                    Instruction::StoreName(name) => {
                        let value = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let env = self.frames[frame_index].env;
                        self.assign_name(env, &name, value.clone())?;
                        self.frames[frame_index].stack.push(value);
                    }
                    Instruction::InitializePattern(pattern) => {
                        let value = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let env = self.frames[frame_index].env;
                        self.initialize_pattern(env, &pattern, value)?;
                    }
                    Instruction::PushEnv => {
                        let current_env = self.frames[frame_index].env;
                        let env = self.new_env(Some(current_env));
                        self.frames[frame_index].scope_stack.push(current_env);
                        self.frames[frame_index].env = env;
                    }
                    Instruction::PopEnv => {
                        let restored = self.frames[frame_index]
                            .scope_stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("scope stack underflow"))?;
                        self.frames[frame_index].env = restored;
                    }
                    Instruction::DeclareName { name, mutable } => {
                        let env = self.frames[frame_index].env;
                        self.declare_name(env, name, mutable)?;
                    }
                    Instruction::MakeClosure { function_id } => {
                        let env = self.frames[frame_index].env;
                        let closure = self.closures.insert(Closure { function_id, env });
                        self.frames[frame_index].stack.push(Value::Closure(closure));
                    }
                    Instruction::MakeArray { count } => {
                        let values = pop_many(&mut self.frames[frame_index].stack, count)?;
                        let array = self.arrays.insert(ArrayObject {
                            elements: values,
                            properties: IndexMap::new(),
                        });
                        self.frames[frame_index].stack.push(Value::Array(array));
                    }
                    Instruction::MakeObject { keys } => {
                        let values = pop_many(&mut self.frames[frame_index].stack, keys.len())?;
                        let mut properties = IndexMap::new();
                        for (key, value) in keys.into_iter().zip(values.into_iter()) {
                            properties.insert(property_name_to_key(&key), value);
                        }
                        let object = self.objects.insert(PlainObject {
                            properties,
                            kind: ObjectKind::Plain,
                        });
                        self.frames[frame_index].stack.push(Value::Object(object));
                    }
                    Instruction::GetPropStatic { name, optional } => {
                        let object = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let value = self.get_property(object, Value::String(name), optional)?;
                        self.frames[frame_index].stack.push(value);
                    }
                    Instruction::GetPropComputed { optional } => {
                        let property = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let object = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let value = self.get_property(object, property, optional)?;
                        self.frames[frame_index].stack.push(value);
                    }
                    Instruction::SetPropStatic { name } => {
                        let value = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let object = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        self.set_property(object, Value::String(name), value.clone())?;
                        self.frames[frame_index].stack.push(value);
                    }
                    Instruction::SetPropComputed => {
                        let value = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let property = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let object = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        self.set_property(object, property, value.clone())?;
                        self.frames[frame_index].stack.push(value);
                    }
                    Instruction::Unary(operator) => {
                        let value = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let result = self.apply_unary(operator, value)?;
                        self.frames[frame_index].stack.push(result);
                    }
                    Instruction::Binary(operator) => {
                        let right = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let left = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let result = self.apply_binary(operator, left, right)?;
                        self.frames[frame_index].stack.push(result);
                    }
                    Instruction::Pop => {
                        self.frames[frame_index].stack.pop();
                    }
                    Instruction::Dup => {
                        let value = self.frames[frame_index]
                            .stack
                            .last()
                            .cloned()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        self.frames[frame_index].stack.push(value);
                    }
                    Instruction::Dup2 => {
                        let len = self.frames[frame_index].stack.len();
                        if len < 2 {
                            return Err(JsliteError::runtime("stack underflow"));
                        }
                        let a = self.frames[frame_index].stack[len - 2].clone();
                        let b = self.frames[frame_index].stack[len - 1].clone();
                        self.frames[frame_index].stack.push(a);
                        self.frames[frame_index].stack.push(b);
                    }
                    Instruction::Jump(target) => self.frames[frame_index].ip = target,
                    Instruction::JumpIfFalse(target) => {
                        let value = self.frames[frame_index]
                            .stack
                            .last()
                            .cloned()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        if !is_truthy(&value) {
                            self.frames[frame_index].ip = target;
                        }
                    }
                    Instruction::JumpIfTrue(target) => {
                        let value = self.frames[frame_index]
                            .stack
                            .last()
                            .cloned()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        if is_truthy(&value) {
                            self.frames[frame_index].ip = target;
                        }
                    }
                    Instruction::JumpIfNullish(target) => {
                        let value = self.frames[frame_index]
                            .stack
                            .last()
                            .cloned()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        if matches!(value, Value::Null | Value::Undefined) {
                            self.frames[frame_index].ip = target;
                        }
                    }
                    Instruction::Call {
                        argc,
                        with_this,
                        optional,
                    } => {
                        let args = pop_many(&mut self.frames[frame_index].stack, argc)?;
                        let callee = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let this_value = if with_this {
                            self.frames[frame_index]
                                .stack
                                .pop()
                                .ok_or_else(|| JsliteError::runtime("stack underflow"))?
                        } else {
                            Value::Undefined
                        };
                        if optional && matches!(callee, Value::Undefined | Value::Null) {
                            self.frames[frame_index].stack.push(Value::Undefined);
                            return Ok(StepAction::Continue);
                        }
                        match self.call_callable(callee, this_value, &args)? {
                            RunState::Completed(value) => {
                                self.frames[frame_index].stack.push(value);
                            }
                            RunState::PushedFrame => {}
                            RunState::Suspended { capability, args } => {
                                return Ok(StepAction::Return(ExecutionStep::Suspended(Box::new(
                                    Suspension {
                                        capability,
                                        args,
                                        snapshot: ExecutionSnapshot {
                                            runtime: self.clone(),
                                        },
                                    },
                                ))));
                            }
                        }
                    }
                    Instruction::Construct { argc } => {
                        let args = pop_many(&mut self.frames[frame_index].stack, argc)?;
                        let callee = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let value = self.construct(callee, &args)?;
                        self.frames[frame_index].stack.push(value);
                    }
                    Instruction::Return => {
                        let value = self.frames[frame_index]
                            .stack
                            .pop()
                            .unwrap_or(Value::Undefined);
                        self.frames.pop();
                        if let Some(parent) = self.frames.last_mut() {
                            parent.stack.push(value);
                        } else {
                            return Ok(StepAction::Return(ExecutionStep::Completed(
                                self.value_to_structured(value)?,
                            )));
                        }
                    }
                }
                Ok(StepAction::Continue)
            })()
            .map_err(|error| self.annotate_runtime_error(error))?;

            match action {
                StepAction::Continue => {}
                StepAction::Return(step) => return Ok(step),
            }
        }
    }

    fn push_frame(&mut self, function_id: usize, env: EnvKey, args: &[Value]) -> JsliteResult<()> {
        let params = self
            .program
            .functions
            .get(function_id)
            .map(|function| function.params.clone())
            .ok_or_else(|| JsliteError::runtime("function not found"))?;
        let this_cell = self.cells.insert(Cell {
            value: Value::Undefined,
            mutable: true,
            initialized: true,
        });
        self.envs
            .get_mut(env)
            .ok_or_else(|| JsliteError::runtime("environment missing"))?
            .bindings
            .insert("this".to_string(), this_cell);
        for pattern in &params {
            for (name, _) in pattern_bindings(pattern) {
                self.declare_name(env, name, true)?;
            }
        }
        for (index, pattern) in params.iter().enumerate() {
            let arg = args.get(index).cloned().unwrap_or(Value::Undefined);
            self.initialize_pattern(env, pattern, arg)?;
        }
        self.frames.push(Frame {
            function_id,
            ip: 0,
            env,
            scope_stack: Vec::new(),
            stack: Vec::new(),
        });
        Ok(())
    }

    fn call_callable(
        &mut self,
        callee: Value,
        _this_value: Value,
        args: &[Value],
    ) -> JsliteResult<RunState> {
        match callee {
            Value::Closure(closure) => {
                let closure = self
                    .closures
                    .get(closure)
                    .cloned()
                    .ok_or_else(|| JsliteError::runtime("closure not found"))?;
                let env = self.new_env(Some(closure.env));
                self.push_frame(closure.function_id, env, args)?;
                Ok(RunState::PushedFrame)
            }
            Value::BuiltinFunction(function) => {
                Ok(RunState::Completed(self.call_builtin(function, args)?))
            }
            Value::HostFunction(capability) => Ok(RunState::Suspended {
                capability,
                args: args
                    .iter()
                    .cloned()
                    .map(|value| self.value_to_structured(value))
                    .collect::<JsliteResult<Vec<_>>>()?,
            }),
            _ => Err(JsliteError::runtime("value is not callable")),
        }
    }

    fn construct(&mut self, callee: Value, args: &[Value]) -> JsliteResult<Value> {
        match callee {
            Value::BuiltinFunction(
                BuiltinFunction::ArrayCtor
                | BuiltinFunction::ObjectCtor
                | BuiltinFunction::NumberCtor
                | BuiltinFunction::StringCtor
                | BuiltinFunction::BooleanCtor,
            ) => self.call_builtin(
                match callee {
                    Value::BuiltinFunction(kind) => kind,
                    _ => unreachable!(),
                },
                args,
            ),
            _ => Err(JsliteError::runtime(
                "only conservative built-in constructors are supported in v1",
            )),
        }
    }

    fn call_builtin(&mut self, function: BuiltinFunction, args: &[Value]) -> JsliteResult<Value> {
        match function {
            BuiltinFunction::ArrayCtor => {
                let array = self.arrays.insert(ArrayObject {
                    elements: args.to_vec(),
                    properties: IndexMap::new(),
                });
                Ok(Value::Array(array))
            }
            BuiltinFunction::ArrayIsArray => {
                Ok(Value::Bool(matches!(args.first(), Some(Value::Array(_)))))
            }
            BuiltinFunction::ObjectCtor => {
                if let Some(Value::Object(object)) = args.first() {
                    Ok(Value::Object(*object))
                } else {
                    let object = self.objects.insert(PlainObject {
                        properties: IndexMap::new(),
                        kind: ObjectKind::Plain,
                    });
                    Ok(Value::Object(object))
                }
            }
            BuiltinFunction::NumberCtor => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?,
            )),
            BuiltinFunction::StringCtor => Ok(Value::String(
                self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?,
            )),
            BuiltinFunction::BooleanCtor => Ok(Value::Bool(is_truthy(
                args.first().unwrap_or(&Value::Undefined),
            ))),
            BuiltinFunction::MathAbs => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                    .abs(),
            )),
            BuiltinFunction::MathMax => {
                let mut value = f64::NEG_INFINITY;
                for arg in args {
                    value = value.max(self.to_number(arg.clone())?);
                }
                Ok(Value::Number(value))
            }
            BuiltinFunction::MathMin => {
                let mut value = f64::INFINITY;
                for arg in args {
                    value = value.min(self.to_number(arg.clone())?);
                }
                Ok(Value::Number(value))
            }
            BuiltinFunction::MathFloor => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                    .floor(),
            )),
            BuiltinFunction::MathCeil => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                    .ceil(),
            )),
            BuiltinFunction::MathRound => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                    .round(),
            )),
            BuiltinFunction::JsonStringify => {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                let structured = self.value_to_structured(value)?;
                let json = serde_json::to_string(&structured_to_json(structured)?)
                    .map_err(|error| JsliteError::runtime(error.to_string()))?;
                Ok(Value::String(json))
            }
            BuiltinFunction::JsonParse => {
                let source = self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
                let parsed: serde_json::Value = serde_json::from_str(&source)
                    .map_err(|error| JsliteError::runtime(error.to_string()))?;
                self.value_from_json(parsed)
            }
        }
    }

    fn install_builtins(&mut self) -> JsliteResult<()> {
        let global_object = self.objects.insert(PlainObject {
            properties: IndexMap::new(),
            kind: ObjectKind::Global,
        });
        self.define_global(
            "globalThis".to_string(),
            Value::Object(global_object),
            false,
        )?;
        self.define_global(
            "Object".to_string(),
            Value::BuiltinFunction(BuiltinFunction::ObjectCtor),
            false,
        )?;
        self.define_global(
            "Array".to_string(),
            Value::BuiltinFunction(BuiltinFunction::ArrayCtor),
            false,
        )?;
        self.define_global(
            "String".to_string(),
            Value::BuiltinFunction(BuiltinFunction::StringCtor),
            false,
        )?;
        self.define_global(
            "Number".to_string(),
            Value::BuiltinFunction(BuiltinFunction::NumberCtor),
            false,
        )?;
        self.define_global(
            "Boolean".to_string(),
            Value::BuiltinFunction(BuiltinFunction::BooleanCtor),
            false,
        )?;

        let math = self.objects.insert(PlainObject {
            properties: IndexMap::from([
                (
                    "abs".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathAbs),
                ),
                (
                    "max".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathMax),
                ),
                (
                    "min".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathMin),
                ),
                (
                    "floor".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathFloor),
                ),
                (
                    "ceil".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathCeil),
                ),
                (
                    "round".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathRound),
                ),
            ]),
            kind: ObjectKind::Math,
        });
        self.define_global("Math".to_string(), Value::Object(math), false)?;

        let json = self.objects.insert(PlainObject {
            properties: IndexMap::from([
                (
                    "stringify".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::JsonStringify),
                ),
                (
                    "parse".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::JsonParse),
                ),
            ]),
            kind: ObjectKind::Json,
        });
        self.define_global("JSON".to_string(), Value::Object(json), false)?;

        let console = self.objects.insert(PlainObject {
            properties: IndexMap::new(),
            kind: ObjectKind::Console,
        });
        self.define_global("console".to_string(), Value::Object(console), false)?;
        Ok(())
    }

    fn new_env(&mut self, parent: Option<EnvKey>) -> EnvKey {
        self.envs.insert(Env {
            parent,
            bindings: IndexMap::new(),
        })
    }

    fn define_global(&mut self, name: String, value: Value, mutable: bool) -> JsliteResult<()> {
        let cell = self.cells.insert(Cell {
            value,
            mutable,
            initialized: true,
        });
        self.envs
            .get_mut(self.globals)
            .ok_or_else(|| JsliteError::runtime("missing globals environment"))?
            .bindings
            .insert(name, cell);
        Ok(())
    }

    fn declare_name(&mut self, env: EnvKey, name: String, mutable: bool) -> JsliteResult<()> {
        let cell = self.cells.insert(Cell {
            value: Value::Undefined,
            mutable,
            initialized: false,
        });
        self.envs
            .get_mut(env)
            .ok_or_else(|| JsliteError::runtime("environment missing"))?
            .bindings
            .entry(name)
            .or_insert(cell);
        Ok(())
    }

    fn lookup_name(&self, env: EnvKey, name: &str) -> JsliteResult<Value> {
        let cell = self
            .find_cell(env, name)
            .ok_or_else(|| JsliteError::Message {
                kind: DiagnosticKind::Runtime,
                message: format!("ReferenceError: `{name}` is not defined"),
                span: None,
                traceback: Vec::new(),
            })?;
        let cell = self
            .cells
            .get(cell)
            .ok_or_else(|| JsliteError::runtime("binding cell missing"))?;
        if !cell.initialized {
            return Err(JsliteError::runtime(format!(
                "ReferenceError: `{name}` accessed before initialization"
            )));
        }
        Ok(cell.value.clone())
    }

    fn assign_name(&mut self, env: EnvKey, name: &str, value: Value) -> JsliteResult<()> {
        let cell_key = self.find_cell(env, name).ok_or_else(|| {
            JsliteError::runtime(format!("ReferenceError: `{name}` is not defined"))
        })?;
        let cell = self
            .cells
            .get_mut(cell_key)
            .ok_or_else(|| JsliteError::runtime("binding cell missing"))?;
        if !cell.initialized {
            return Err(JsliteError::runtime(format!(
                "ReferenceError: `{name}` accessed before initialization"
            )));
        }
        if !cell.mutable {
            return Err(JsliteError::runtime(format!(
                "TypeError: assignment to constant variable `{name}`"
            )));
        }
        cell.value = value;
        Ok(())
    }

    fn initialize_name_in_env(
        &mut self,
        env: EnvKey,
        name: &str,
        value: Value,
    ) -> JsliteResult<()> {
        let cell_key = self
            .envs
            .get(env)
            .and_then(|env| env.bindings.get(name).copied())
            .ok_or_else(|| {
                JsliteError::runtime(format!("binding `{name}` missing in current scope"))
            })?;
        let cell = self
            .cells
            .get_mut(cell_key)
            .ok_or_else(|| JsliteError::runtime("binding cell missing"))?;
        if cell.initialized {
            if !cell.mutable {
                return Err(JsliteError::runtime(format!(
                    "TypeError: binding `{name}` was already initialized"
                )));
            }
            cell.value = value;
            return Ok(());
        }
        cell.value = value;
        cell.initialized = true;
        Ok(())
    }

    fn find_cell(&self, env: EnvKey, name: &str) -> Option<CellKey> {
        let mut current = Some(env);
        while let Some(key) = current {
            let env = self.envs.get(key)?;
            if let Some(cell) = env.bindings.get(name) {
                return Some(*cell);
            }
            current = env.parent;
        }
        None
    }

    fn initialize_pattern(
        &mut self,
        env: EnvKey,
        pattern: &Pattern,
        value: Value,
    ) -> JsliteResult<()> {
        match pattern {
            Pattern::Identifier { name, .. } => self.initialize_name_in_env(env, name, value),
            Pattern::Default {
                target,
                default_value,
                ..
            } => {
                let value = if matches!(value, Value::Undefined) {
                    let bytecode = BytecodeProgram {
                        functions: vec![FunctionPrototype {
                            name: None,
                            params: Vec::new(),
                            code: Vec::new(),
                            is_async: false,
                            span: SourceSpan::new(0, 0),
                        }],
                        root: 0,
                    };
                    drop(bytecode);
                    return Err(JsliteError::runtime(format!(
                        "default pattern initialization at runtime requires compiled evaluation support: {:?}",
                        default_value
                    )));
                } else {
                    value
                };
                self.initialize_pattern(env, target, value)
            }
            Pattern::Array { elements, rest, .. } => {
                let items = self.to_array_items(value)?;
                for (index, pattern) in elements.iter().enumerate() {
                    if let Some(pattern) = pattern {
                        self.initialize_pattern(
                            env,
                            pattern,
                            items.get(index).cloned().unwrap_or(Value::Undefined),
                        )?;
                    }
                }
                if let Some(rest) = rest {
                    let array = self.arrays.insert(ArrayObject {
                        elements: items.into_iter().skip(elements.len()).collect(),
                        properties: IndexMap::new(),
                    });
                    self.initialize_pattern(env, rest, Value::Array(array))?;
                }
                Ok(())
            }
            Pattern::Object {
                properties, rest, ..
            } => {
                let mut seen = HashSet::new();
                for property in properties {
                    let key = property_name_to_key(&property.key);
                    let prop_value =
                        self.get_property(value.clone(), Value::String(key.clone()), false)?;
                    seen.insert(key);
                    self.initialize_pattern(env, &property.value, prop_value)?;
                }
                if let Some(rest_pattern) = rest {
                    let mut rest_object = IndexMap::new();
                    match value {
                        Value::Object(object) => {
                            if let Some(object) = self.objects.get(object) {
                                for (key, value) in &object.properties {
                                    if !seen.contains(key) {
                                        rest_object.insert(key.clone(), value.clone());
                                    }
                                }
                            }
                        }
                        Value::Array(array) => {
                            if let Some(array) = self.arrays.get(array) {
                                for (index, value) in array.elements.iter().enumerate() {
                                    let key = index.to_string();
                                    if !seen.contains(&key) {
                                        rest_object.insert(key, value.clone());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    let rest = self.objects.insert(PlainObject {
                        properties: rest_object,
                        kind: ObjectKind::Plain,
                    });
                    self.initialize_pattern(env, rest_pattern, Value::Object(rest))?;
                }
                Ok(())
            }
        }
    }

    fn get_property(&self, object: Value, property: Value, optional: bool) -> JsliteResult<Value> {
        if optional && matches!(object, Value::Null | Value::Undefined) {
            return Ok(Value::Undefined);
        }
        let key = self.to_property_key(property)?;
        match object {
            Value::Object(object) => {
                let object = self
                    .objects
                    .get(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?;
                Ok(object
                    .properties
                    .get(&key)
                    .cloned()
                    .unwrap_or(Value::Undefined))
            }
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                if key == "length" {
                    Ok(Value::Number(array.elements.len() as f64))
                } else if let Ok(index) = key.parse::<usize>() {
                    Ok(array
                        .elements
                        .get(index)
                        .cloned()
                        .unwrap_or(Value::Undefined))
                } else {
                    Ok(array
                        .properties
                        .get(&key)
                        .cloned()
                        .unwrap_or(Value::Undefined))
                }
            }
            Value::BuiltinFunction(BuiltinFunction::ArrayCtor) if key == "isArray" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::ArrayIsArray))
            }
            Value::String(value) if key == "length" => {
                Ok(Value::Number(value.chars().count() as f64))
            }
            Value::Null | Value::Undefined => Err(JsliteError::runtime(
                "TypeError: cannot read properties of nullish value",
            )),
            _ => Ok(Value::Undefined),
        }
    }

    fn set_property(&mut self, object: Value, property: Value, value: Value) -> JsliteResult<()> {
        let key = self.to_property_key(property)?;
        match object {
            Value::Object(object) => {
                let object = self
                    .objects
                    .get_mut(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?;
                object.properties.insert(key, value);
                Ok(())
            }
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get_mut(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                if let Ok(index) = key.parse::<usize>() {
                    if index >= array.elements.len() {
                        array.elements.resize(index + 1, Value::Undefined);
                    }
                    array.elements[index] = value;
                } else {
                    array.properties.insert(key, value);
                }
                Ok(())
            }
            _ => Err(JsliteError::runtime("TypeError: value is not an object")),
        }
    }

    fn apply_unary(&mut self, operator: UnaryOp, value: Value) -> JsliteResult<Value> {
        match operator {
            UnaryOp::Plus => Ok(Value::Number(self.to_number(value)?)),
            UnaryOp::Minus => Ok(Value::Number(-self.to_number(value)?)),
            UnaryOp::Not => Ok(Value::Bool(!is_truthy(&value))),
            UnaryOp::Typeof => Ok(Value::String(
                match value {
                    Value::Undefined => "undefined",
                    Value::Null => "object",
                    Value::Bool(_) => "boolean",
                    Value::Number(_) => "number",
                    Value::String(_) => "string",
                    Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_) => {
                        "function"
                    }
                    Value::Object(_) | Value::Array(_) => "object",
                }
                .to_string(),
            )),
            UnaryOp::Void => Ok(Value::Undefined),
        }
    }

    fn apply_binary(
        &mut self,
        operator: BinaryOp,
        left: Value,
        right: Value,
    ) -> JsliteResult<Value> {
        match operator {
            BinaryOp::Add => {
                if matches!(left, Value::String(_)) || matches!(right, Value::String(_)) {
                    Ok(Value::String(format!(
                        "{}{}",
                        self.to_string(left)?,
                        self.to_string(right)?
                    )))
                } else {
                    Ok(Value::Number(
                        self.to_number(left)? + self.to_number(right)?,
                    ))
                }
            }
            BinaryOp::Sub => Ok(Value::Number(
                self.to_number(left)? - self.to_number(right)?,
            )),
            BinaryOp::Mul => Ok(Value::Number(
                self.to_number(left)? * self.to_number(right)?,
            )),
            BinaryOp::Div => Ok(Value::Number(
                self.to_number(left)? / self.to_number(right)?,
            )),
            BinaryOp::Rem => Ok(Value::Number(
                self.to_number(left)? % self.to_number(right)?,
            )),
            BinaryOp::Eq | BinaryOp::StrictEq => Ok(Value::Bool(strict_equal(&left, &right))),
            BinaryOp::NotEq | BinaryOp::StrictNotEq => {
                Ok(Value::Bool(!strict_equal(&left, &right)))
            }
            BinaryOp::LessThan => Ok(Value::Bool(self.to_number(left)? < self.to_number(right)?)),
            BinaryOp::LessThanEq => {
                Ok(Value::Bool(self.to_number(left)? <= self.to_number(right)?))
            }
            BinaryOp::GreaterThan => {
                Ok(Value::Bool(self.to_number(left)? > self.to_number(right)?))
            }
            BinaryOp::GreaterThanEq => {
                Ok(Value::Bool(self.to_number(left)? >= self.to_number(right)?))
            }
        }
    }
    fn to_number(&self, value: Value) -> JsliteResult<f64> {
        Ok(match value {
            Value::Undefined => f64::NAN,
            Value::Null => 0.0,
            Value::Bool(value) => {
                if value {
                    1.0
                } else {
                    0.0
                }
            }
            Value::Number(value) => value,
            Value::String(value) => value.parse::<f64>().unwrap_or(f64::NAN),
            Value::Array(_)
            | Value::Object(_)
            | Value::Closure(_)
            | Value::BuiltinFunction(_)
            | Value::HostFunction(_) => {
                return Err(JsliteError::runtime(
                    "cannot coerce complex value to number",
                ));
            }
        })
    }

    fn to_string(&self, value: Value) -> JsliteResult<String> {
        Ok(match value {
            Value::Undefined => "undefined".to_string(),
            Value::Null => "null".to_string(),
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => {
                if value.fract() == 0.0 {
                    format!("{}", value as i64)
                } else {
                    value.to_string()
                }
            }
            Value::String(value) => value,
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                let mut parts = Vec::new();
                for value in &array.elements {
                    parts.push(self.to_string(value.clone())?);
                }
                parts.join(",")
            }
            Value::Object(_) => "[object Object]".to_string(),
            Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_) => {
                "[Function]".to_string()
            }
        })
    }

    fn to_property_key(&self, value: Value) -> JsliteResult<String> {
        match value {
            Value::String(value) => Ok(value),
            Value::Number(value) => Ok(format_number_key(value)),
            Value::Bool(value) => Ok(value.to_string()),
            Value::Null => Ok("null".to_string()),
            Value::Undefined => Ok("undefined".to_string()),
            _ => self.to_string(value),
        }
    }

    fn to_array_items(&self, value: Value) -> JsliteResult<Vec<Value>> {
        match value {
            Value::Array(array) => self
                .arrays
                .get(array)
                .map(|array| array.elements.clone())
                .ok_or_else(|| JsliteError::runtime("array missing")),
            Value::Undefined | Value::Null => Ok(Vec::new()),
            _ => Err(JsliteError::runtime(
                "value is not destructurable as an array",
            )),
        }
    }

    fn bump_instruction_budget(&mut self) -> JsliteResult<()> {
        self.instruction_counter += 1;
        if self.instruction_counter > self.limits.instruction_budget {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Limit,
                message: "instruction budget exhausted".to_string(),
                span: None,
                traceback: Vec::new(),
            });
        }
        Ok(())
    }

    fn value_from_structured(&mut self, value: StructuredValue) -> JsliteResult<Value> {
        Ok(match value {
            StructuredValue::Undefined => Value::Undefined,
            StructuredValue::Null => Value::Null,
            StructuredValue::Bool(value) => Value::Bool(value),
            StructuredValue::String(value) => Value::String(value),
            StructuredValue::Number(number) => Value::Number(number.to_f64()),
            StructuredValue::Array(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.value_from_structured(item)?);
                }
                let array = self.arrays.insert(ArrayObject {
                    elements: values,
                    properties: IndexMap::new(),
                });
                Value::Array(array)
            }
            StructuredValue::Object(object) => {
                let mut properties = IndexMap::new();
                for (key, value) in object {
                    properties.insert(key, self.value_from_structured(value)?);
                }
                let object = self.objects.insert(PlainObject {
                    properties,
                    kind: ObjectKind::Plain,
                });
                Value::Object(object)
            }
        })
    }

    fn value_to_structured(&self, value: Value) -> JsliteResult<StructuredValue> {
        Ok(match value {
            Value::Undefined => StructuredValue::Undefined,
            Value::Null => StructuredValue::Null,
            Value::Bool(value) => StructuredValue::Bool(value),
            Value::Number(value) => StructuredValue::Number(StructuredNumber::from_f64(value)),
            Value::String(value) => StructuredValue::String(value),
            Value::Array(array) => StructuredValue::Array(
                self.arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?
                    .elements
                    .iter()
                    .cloned()
                    .map(|value| self.value_to_structured(value))
                    .collect::<JsliteResult<Vec<_>>>()?,
            ),
            Value::Object(object) => StructuredValue::Object(
                self.objects
                    .get(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?
                    .properties
                    .iter()
                    .map(|(key, value)| Ok((key.clone(), self.value_to_structured(value.clone())?)))
                    .collect::<JsliteResult<IndexMap<_, _>>>()?,
            ),
            Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_) => {
                return Err(JsliteError::runtime(
                    "functions cannot cross the structured host boundary",
                ));
            }
        })
    }

    fn value_from_json(&mut self, value: serde_json::Value) -> JsliteResult<Value> {
        match value {
            serde_json::Value::Null => Ok(Value::Null),
            serde_json::Value::Bool(value) => Ok(Value::Bool(value)),
            serde_json::Value::Number(number) => Ok(Value::Number(number.as_f64().unwrap_or(0.0))),
            serde_json::Value::String(value) => Ok(Value::String(value)),
            serde_json::Value::Array(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.value_from_json(item)?);
                }
                let array = self.arrays.insert(ArrayObject {
                    elements: values,
                    properties: IndexMap::new(),
                });
                Ok(Value::Array(array))
            }
            serde_json::Value::Object(object) => {
                let mut properties = IndexMap::new();
                for (key, value) in object {
                    properties.insert(key, self.value_from_json(value)?);
                }
                let object = self.objects.insert(PlainObject {
                    properties,
                    kind: ObjectKind::Plain,
                });
                Ok(Value::Object(object))
            }
        }
    }
}

fn pop_many(stack: &mut Vec<Value>, count: usize) -> JsliteResult<Vec<Value>> {
    if stack.len() < count {
        return Err(JsliteError::runtime("stack underflow"));
    }
    let start = stack.len() - count;
    Ok(stack.drain(start..).collect())
}

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Undefined | Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => *value != 0.0 && !value.is_nan(),
        Value::String(value) => !value.is_empty(),
        Value::Object(_)
        | Value::Array(_)
        | Value::Closure(_)
        | Value::BuiltinFunction(_)
        | Value::HostFunction(_) => true,
    }
}

fn strict_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Undefined, Value::Undefined) => true,
        (Value::Null, Value::Null) => true,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::Number(left), Value::Number(right)) => left == right,
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Object(left), Value::Object(right)) => left == right,
        (Value::Array(left), Value::Array(right)) => left == right,
        (Value::Closure(left), Value::Closure(right)) => left == right,
        (Value::BuiltinFunction(left), Value::BuiltinFunction(right)) => left == right,
        _ => false,
    }
}

fn property_name_to_key(name: &PropertyName) -> String {
    match name {
        PropertyName::Identifier(name) | PropertyName::String(name) => name.clone(),
        PropertyName::Number(number) => format_number_key(*number),
    }
}

fn format_number_key(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

fn structured_to_json(value: StructuredValue) -> JsliteResult<serde_json::Value> {
    Ok(match value {
        StructuredValue::Undefined => serde_json::Value::Null,
        StructuredValue::Null => serde_json::Value::Null,
        StructuredValue::Bool(value) => serde_json::Value::Bool(value),
        StructuredValue::String(value) => serde_json::Value::String(value),
        StructuredValue::Number(number) => match number {
            StructuredNumber::Finite(value) => serde_json::Number::from_f64(value)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            StructuredNumber::NaN
            | StructuredNumber::Infinity
            | StructuredNumber::NegInfinity
            | StructuredNumber::NegZero => serde_json::Value::Null,
        },
        StructuredValue::Array(values) => serde_json::Value::Array(
            values
                .into_iter()
                .map(structured_to_json)
                .collect::<JsliteResult<Vec<_>>>()?,
        ),
        StructuredValue::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| Ok((key, structured_to_json(value)?)))
                .collect::<JsliteResult<serde_json::Map<_, _>>>()?,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile;

    fn test_function(code: Vec<Instruction>) -> FunctionPrototype {
        FunctionPrototype {
            name: None,
            params: Vec::new(),
            code,
            is_async: false,
            span: SourceSpan::new(0, 0),
        }
    }

    fn invalid_program(code: Vec<Instruction>) -> BytecodeProgram {
        BytecodeProgram {
            functions: vec![test_function(code)],
            root: 0,
        }
    }

    fn run(source: &str) -> StructuredValue {
        let program = compile(source).expect("source should compile");
        execute(&program, ExecutionOptions::default()).expect("program should run")
    }

    #[test]
    fn runs_arithmetic_and_locals() {
        let value = run(r#"
            const a = 4;
            const b = 3;
            a * b + 2;
            "#);
        assert_eq!(
            value,
            StructuredValue::Number(StructuredNumber::Finite(14.0))
        );
    }

    #[test]
    fn runs_functions_and_closures() {
        let value = run(r#"
            function makeAdder(x) {
              return (y) => x + y;
            }
            const add2 = makeAdder(2);
            add2(5);
            "#);
        assert_eq!(
            value,
            StructuredValue::Number(StructuredNumber::Finite(7.0))
        );
    }

    #[test]
    fn runs_arrays_objects_and_member_access() {
        let value = run(r#"
            const values = [1, 2];
            const record = { total: values[0] + values[1] };
            record.total;
            "#);
        assert_eq!(
            value,
            StructuredValue::Number(StructuredNumber::Finite(3.0))
        );
    }

    #[test]
    fn runs_branching_loops_and_switch() {
        let value = run(r#"
            let total = 0;
            let i = 0;
            while (i < 4) {
              total += i;
              i += 1;
            }
            do {
              total += 1;
            } while (false);
            for (let j = 0; j < 2; j += 1) {
              if (j === 0) {
                continue;
              }
              total += j;
            }
            switch (total) {
              case 8:
                total += 1;
                break;
              default:
                total = 0;
            }
            total;
            "#);
        assert_eq!(
            value,
            StructuredValue::Number(StructuredNumber::Finite(9.0))
        );
    }

    #[test]
    fn runs_math_and_json_builtins() {
        let value = run(r#"
            const encoded = JSON.stringify({ value: Math.max(1, 9, 4) });
            JSON.parse(encoded).value;
            "#);
        assert_eq!(
            value,
            StructuredValue::Number(StructuredNumber::Finite(9.0))
        );
    }

    #[test]
    fn enforces_instruction_budget() {
        let program = compile("while (true) {}").expect("source should compile");
        let error = execute(
            &program,
            ExecutionOptions {
                inputs: IndexMap::new(),
                capabilities: Vec::new(),
                limits: RuntimeLimits {
                    instruction_budget: 100,
                    ..RuntimeLimits::default()
                },
            },
        )
        .expect_err("infinite loop should exhaust budget");
        assert!(error.to_string().contains("instruction budget exhausted"));
    }

    #[test]
    fn lowering_errors_preserve_source_spans() {
        let program = compile("continue;").expect("source should compile");
        let error =
            lower_to_bytecode(&program).expect_err("continue outside a loop should fail lowering");
        let rendered = error.to_string();
        assert!(rendered.contains("`continue` used outside of a loop"));
        assert!(rendered.contains("[0..9]"));
    }

    #[test]
    fn runtime_errors_include_guest_tracebacks() {
        let program = compile(
            r#"
            function outer() {
              return inner();
            }
            function inner() {
              const value = null;
              return value.answer;
            }
            outer();
            "#,
        )
        .expect("source should compile");
        let error = execute(&program, ExecutionOptions::default())
            .expect_err("nullish property access should fail");
        let rendered = error.to_string();
        assert!(rendered.contains("TypeError: cannot read properties of nullish value"));
        assert!(rendered.contains("at inner ["));
        assert!(rendered.contains("at outer ["));
        assert!(rendered.contains("at <script> ["));
        assert!(!rendered.contains(".rs"));
    }

    #[test]
    fn suspends_and_resumes_host_capability_calls() {
        let program = compile(
            r#"
            const value = fetch_data(41);
            value + 1;
            "#,
        )
        .expect("source should compile");

        let step = start(
            &program,
            ExecutionOptions {
                capabilities: vec!["fetch_data".to_string()],
                ..ExecutionOptions::default()
            },
        )
        .expect("execution should start");

        let suspension = match step {
            ExecutionStep::Suspended(suspension) => suspension,
            other => panic!("expected suspension, got {other:?}"),
        };
        assert_eq!(suspension.capability, "fetch_data");
        assert_eq!(
            suspension.args,
            vec![StructuredValue::Number(StructuredNumber::Finite(41.0))]
        );

        let resumed = resume(
            suspension.snapshot,
            ResumePayload::Value(StructuredValue::Number(StructuredNumber::Finite(41.0))),
        )
        .expect("resume should succeed");

        match resumed {
            ExecutionStep::Completed(value) => {
                assert_eq!(
                    value,
                    StructuredValue::Number(StructuredNumber::Finite(42.0))
                );
            }
            other => panic!("expected completion, got {other:?}"),
        }
    }

    #[test]
    fn round_trips_program_and_snapshot() {
        let program =
            compile("const value = fetch_data(1); value + 2;").expect("compile should succeed");
        let bytecode = lower_to_bytecode(&program).expect("lowering should succeed");
        let program_bytes = dump_program(&bytecode).expect("program dump should succeed");
        let loaded_program = load_program(&program_bytes).expect("program load should succeed");
        assert_eq!(loaded_program.root, bytecode.root);
        assert_eq!(loaded_program.functions.len(), bytecode.functions.len());

        let step = start(
            &program,
            ExecutionOptions {
                capabilities: vec!["fetch_data".to_string()],
                ..ExecutionOptions::default()
            },
        )
        .expect("execution should suspend");
        let suspension = match step {
            ExecutionStep::Suspended(suspension) => suspension,
            other => panic!("expected suspension, got {other:?}"),
        };
        let snapshot_bytes =
            dump_snapshot(&suspension.snapshot).expect("snapshot dump should succeed");
        let loaded_snapshot = load_snapshot(&snapshot_bytes).expect("snapshot load should succeed");
        let resumed = resume(
            loaded_snapshot,
            ResumePayload::Value(StructuredValue::Number(StructuredNumber::Finite(1.0))),
        )
        .expect("resume should succeed");
        match resumed {
            ExecutionStep::Completed(value) => {
                assert_eq!(
                    value,
                    StructuredValue::Number(StructuredNumber::Finite(3.0))
                );
            }
            other => panic!("expected completion, got {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_jump_targets_before_execution() {
        let program = invalid_program(vec![Instruction::Jump(99), Instruction::Return]);
        let error = start_bytecode(&program, ExecutionOptions::default())
            .expect_err("invalid jump target should fail validation");
        assert!(error.to_string().contains("jumps to invalid target 99"));
    }

    #[test]
    fn rejects_inconsistent_stack_depth_in_serialized_programs() {
        let program = invalid_program(vec![
            Instruction::PushNumber(1.0),
            Instruction::JumpIfTrue(3),
            Instruction::Pop,
            Instruction::Return,
        ]);
        let bytes = dump_program(&program).expect("invalid program still serializes");
        let error =
            load_program(&bytes).expect_err("invalid serialized program should fail validation");
        assert!(
            error
                .to_string()
                .contains("has inconsistent stack or scope depth")
        );
    }

    #[test]
    fn rejects_cross_version_serialized_programs() {
        let program = lower_to_bytecode(&compile("1;").expect("compile should succeed"))
            .expect("lowering should succeed");
        let mut bytes = dump_program(&program).expect("program should serialize");
        bytes[0] = bytes[0].saturating_add(1);
        let error = load_program(&bytes).expect_err("cross-version program should be rejected");
        assert!(
            error
                .to_string()
                .contains("serialized program version mismatch")
        );
    }

    #[test]
    fn rejects_invalid_snapshot_frame_state() {
        let program =
            compile("const value = fetch_data(1); value + 2;").expect("compile should succeed");
        let step = start(
            &program,
            ExecutionOptions {
                capabilities: vec!["fetch_data".to_string()],
                ..ExecutionOptions::default()
            },
        )
        .expect("execution should suspend");
        let mut suspension = match step {
            ExecutionStep::Suspended(suspension) => *suspension,
            other => panic!("expected suspension, got {other:?}"),
        };
        suspension.snapshot.runtime.frames[0].ip = 999;
        let bytes = dump_snapshot(&suspension.snapshot).expect("snapshot should serialize");
        let error = load_snapshot(&bytes).expect_err("invalid snapshot should fail validation");
        assert!(
            error
                .to_string()
                .contains("frame instruction pointer 999 is out of range")
        );
    }

    #[test]
    fn rejects_cross_version_snapshots() {
        let program =
            compile("const value = fetch_data(1); value + 2;").expect("compile should succeed");
        let step = start(
            &program,
            ExecutionOptions {
                capabilities: vec!["fetch_data".to_string()],
                ..ExecutionOptions::default()
            },
        )
        .expect("execution should suspend");
        let suspension = match step {
            ExecutionStep::Suspended(suspension) => suspension,
            other => panic!("expected suspension, got {other:?}"),
        };
        let mut bytes = dump_snapshot(&suspension.snapshot).expect("snapshot should serialize");
        bytes[0] = bytes[0].saturating_add(1);
        let error = load_snapshot(&bytes).expect_err("cross-version snapshot should be rejected");
        assert!(
            error
                .to_string()
                .contains("serialized snapshot version mismatch")
        );
    }
}
