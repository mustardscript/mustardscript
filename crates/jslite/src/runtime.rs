use std::collections::{HashSet, VecDeque};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use slotmap::{SlotMap, new_key_type};

use crate::{
    cancellation::CancellationToken,
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
new_key_type! { struct MapKey; }
new_key_type! { struct SetKey; }
new_key_type! { struct IteratorKey; }
new_key_type! { struct ClosureKey; }
new_key_type! { struct PromiseKey; }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionOptions {
    pub inputs: IndexMap<String, StructuredValue>,
    pub capabilities: Vec<String>,
    pub limits: RuntimeLimits,
    #[serde(skip, default)]
    pub cancellation_token: Option<CancellationToken>,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            inputs: IndexMap::new(),
            capabilities: Vec::new(),
            limits: RuntimeLimits::default(),
            cancellation_token: None,
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
    Cancelled,
}

#[derive(Debug, Clone, Default)]
pub struct ResumeOptions {
    pub cancellation_token: Option<CancellationToken>,
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
    pub rest: Option<Pattern>,
    pub code: Vec<Instruction>,
    pub is_async: bool,
    pub is_arrow: bool,
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
    CreateIterator,
    IteratorNext,
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
    PushHandler {
        catch: Option<usize>,
        finally: Option<usize>,
    },
    PopHandler,
    EnterFinally {
        exit: usize,
    },
    BeginCatch,
    Throw {
        span: SourceSpan,
    },
    PushPendingJump {
        target: usize,
        target_handler_depth: usize,
        target_scope_depth: usize,
    },
    PushPendingReturn,
    PushPendingThrow,
    ContinuePending,
    Jump(usize),
    JumpIfFalse(usize),
    JumpIfTrue(usize),
    JumpIfNullish(usize),
    Call {
        argc: usize,
        with_this: bool,
        optional: bool,
    },
    Await,
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
    resume_with_options(snapshot, payload, ResumeOptions::default())
}

pub fn resume_with_options(
    snapshot: ExecutionSnapshot,
    payload: ResumePayload,
    options: ResumeOptions,
) -> JsliteResult<ExecutionStep> {
    let mut runtime = snapshot.runtime;
    runtime.apply_resume_options(options);
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
    let mut snapshot = decoded.snapshot;
    validate_snapshot(&snapshot)?;
    snapshot.runtime.recompute_accounting_after_load()?;
    Ok(snapshot)
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
    handler_depth: usize,
    pending_depth: usize,
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
            | Instruction::EnterFinally { exit: target }
            | Instruction::PushPendingJump { target, .. }
                if *target >= code_len =>
            {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} jumps to invalid target {target}"
                    ),
                    None,
                ));
            }
            Instruction::PushHandler { catch, finally }
                if catch.is_some_and(|target| target >= code_len)
                    || finally.is_some_and(|target| target >= code_len) =>
            {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} references an invalid exception target"
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
            handler_depth: 0,
            pending_depth: 0,
        },
    )]);
    while let Some((ip, state)) = work.pop_front() {
        if let Some(existing) = states[ip] {
            if existing != state {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} has inconsistent validation state: existing={existing:?}, new={state:?}"
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
        match instruction {
            Instruction::PushHandler { catch, finally } => {
                if let Some(target) = catch {
                    work.push_back((
                        *target,
                        ValidationState {
                            handler_depth: state.handler_depth,
                            ..state
                        },
                    ));
                } else if let Some(target) = finally {
                    work.push_back((
                        *target,
                        ValidationState {
                            handler_depth: state.handler_depth,
                            pending_depth: state.pending_depth + 1,
                            ..state
                        },
                    ));
                }
            }
            Instruction::PushPendingJump {
                target,
                target_handler_depth,
                target_scope_depth,
            } => {
                work.push_back((
                    *target,
                    ValidationState {
                        scope_depth: *target_scope_depth,
                        handler_depth: *target_handler_depth,
                        ..state
                    },
                ));
            }
            _ => {}
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
        | Instruction::MakeClosure { .. }
        | Instruction::BeginCatch => ValidationState {
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
        Instruction::CreateIterator => {
            require_stack(1)?;
            state
        }
        Instruction::IteratorNext => {
            require_stack(1)?;
            ValidationState {
                stack_depth: state.stack_depth + 1,
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
        Instruction::PushHandler { .. } => ValidationState {
            handler_depth: state.handler_depth + 1,
            ..state
        },
        Instruction::PopHandler => {
            if state.handler_depth == 0 {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} pops an empty handler stack"
                    ),
                    None,
                ));
            }
            ValidationState {
                handler_depth: state.handler_depth - 1,
                ..state
            }
        }
        Instruction::EnterFinally { .. } => state,
        Instruction::Throw { .. } => {
            require_stack(1)?;
            state
        }
        Instruction::PushPendingJump {
            target_handler_depth,
            target_scope_depth,
            ..
        } => {
            if *target_handler_depth > state.handler_depth {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} targets handler depth {target_handler_depth} from depth {}",
                        state.handler_depth
                    ),
                    None,
                ));
            }
            if *target_scope_depth > state.scope_depth {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} targets scope depth {target_scope_depth} from depth {}",
                        state.scope_depth
                    ),
                    None,
                ));
            }
            ValidationState {
                pending_depth: state.pending_depth + 1,
                ..state
            }
        }
        Instruction::PushPendingReturn => {
            require_stack(1)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
                pending_depth: state.pending_depth + 1,
                ..state
            }
        }
        Instruction::PushPendingThrow => {
            require_stack(1)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
                pending_depth: state.pending_depth + 1,
                ..state
            }
        }
        Instruction::ContinuePending => {
            if state.pending_depth == 0 {
                return Err(JsliteError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} resumes without a pending completion"
                    ),
                    None,
                ));
            }
            ValidationState {
                pending_depth: state.pending_depth - 1,
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
        Instruction::Await => {
            require_stack(1)?;
            state
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
        Instruction::ContinuePending | Instruction::Throw { .. } | Instruction::Return => {
            Vec::new()
        }
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
    if runtime.frames.len() > runtime.limits.call_depth_limit {
        return Err(limit_error("call depth limit exceeded"));
    }
    if runtime.frames.is_empty()
        && runtime.suspended_host_call.is_none()
        && runtime.root_result.is_none()
        && runtime.microtasks.is_empty()
    {
        return Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: "snapshot validation failed: suspended runtime has no frames or async state"
                .to_string(),
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
        if let Some(value) = &frame.pending_exception {
            validate_runtime_value(runtime, value)?;
        }
        for handler in &frame.handlers {
            if let Some(catch) = handler.catch
                && catch >= function.code.len()
            {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message: format!(
                        "snapshot validation failed: frame handler catch target {} is out of range for function {}",
                        catch, frame.function_id
                    ),
                    span: None,
                    traceback: Vec::new(),
                });
            }
            if let Some(finally) = handler.finally
                && finally >= function.code.len()
            {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message: format!(
                        "snapshot validation failed: frame handler finally target {} is out of range for function {}",
                        finally, frame.function_id
                    ),
                    span: None,
                    traceback: Vec::new(),
                });
            }
            if runtime.envs.get(handler.env).is_none() {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message: format!(
                        "snapshot validation failed: frame handler references missing environment {:?}",
                        handler.env
                    ),
                    span: None,
                    traceback: Vec::new(),
                });
            }
            if handler.scope_stack_len > frame.scope_stack.len()
                || handler.stack_len > frame.stack.len()
            {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message: "snapshot validation failed: frame handler restore state exceeds the current frame state".to_string(),
                    span: None,
                    traceback: Vec::new(),
                });
            }
        }
        for completion in &frame.pending_completions {
            match completion {
                CompletionRecord::Jump {
                    target,
                    target_handler_depth,
                    target_scope_depth,
                } => {
                    if *target >= function.code.len() {
                        return Err(JsliteError::Message {
                            kind: DiagnosticKind::Serialization,
                            message: format!(
                                "snapshot validation failed: pending jump target {} is out of range for function {}",
                                target, frame.function_id
                            ),
                            span: None,
                            traceback: Vec::new(),
                        });
                    }
                    if *target_handler_depth > frame.handlers.len() {
                        return Err(JsliteError::Message {
                            kind: DiagnosticKind::Serialization,
                            message: format!(
                                "snapshot validation failed: pending jump targets handler depth {} but only {} handlers are active",
                                target_handler_depth,
                                frame.handlers.len()
                            ),
                            span: None,
                            traceback: Vec::new(),
                        });
                    }
                    if *target_scope_depth > frame.scope_stack.len() {
                        return Err(JsliteError::Message {
                            kind: DiagnosticKind::Serialization,
                            message: format!(
                                "snapshot validation failed: pending jump targets scope depth {} but only {} scopes are active",
                                target_scope_depth,
                                frame.scope_stack.len()
                            ),
                            span: None,
                            traceback: Vec::new(),
                        });
                    }
                }
                CompletionRecord::Return(value) | CompletionRecord::Throw(value) => {
                    validate_runtime_value(runtime, value)?;
                }
            }
        }
        for active in &frame.active_finally {
            if active.completion_index >= frame.pending_completions.len() {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message:
                        "snapshot validation failed: active finally references a missing completion"
                            .to_string(),
                    span: None,
                    traceback: Vec::new(),
                });
            }
            if active.exit >= function.code.len() {
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Serialization,
                    message: format!(
                        "snapshot validation failed: active finally exit target {} is out of range for function {}",
                        active.exit, frame.function_id
                    ),
                    span: None,
                    traceback: Vec::new(),
                });
            }
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
    for map in runtime.maps.values() {
        for entry in &map.entries {
            validate_runtime_value(runtime, &entry.key)?;
            validate_runtime_value(runtime, &entry.value)?;
        }
    }
    for set in runtime.sets.values() {
        for value in &set.entries {
            validate_runtime_value(runtime, value)?;
        }
    }
    for iterator in runtime.iterators.values() {
        match iterator.state {
            IteratorState::Array(ref state) => {
                if runtime.arrays.get(state.array).is_none() {
                    return Err(JsliteError::Message {
                        kind: DiagnosticKind::Serialization,
                        message: format!(
                            "snapshot validation failed: iterator references missing array {:?}",
                            state.array
                        ),
                        span: None,
                        traceback: Vec::new(),
                    });
                }
            }
            IteratorState::ArrayKeys(ref state) | IteratorState::ArrayEntries(ref state) => {
                if runtime.arrays.get(state.array).is_none() {
                    return Err(JsliteError::Message {
                        kind: DiagnosticKind::Serialization,
                        message: format!(
                            "snapshot validation failed: iterator references missing array {:?}",
                            state.array
                        ),
                        span: None,
                        traceback: Vec::new(),
                    });
                }
            }
            IteratorState::String(_) => {}
            IteratorState::MapEntries(ref state)
            | IteratorState::MapKeys(ref state)
            | IteratorState::MapValues(ref state) => {
                if runtime.maps.get(state.map).is_none() {
                    return Err(JsliteError::Message {
                        kind: DiagnosticKind::Serialization,
                        message: format!(
                            "snapshot validation failed: iterator references missing map {:?}",
                            state.map
                        ),
                        span: None,
                        traceback: Vec::new(),
                    });
                }
            }
            IteratorState::SetEntries(ref state) | IteratorState::SetValues(ref state) => {
                if runtime.sets.get(state.set).is_none() {
                    return Err(JsliteError::Message {
                        kind: DiagnosticKind::Serialization,
                        message: format!(
                            "snapshot validation failed: iterator references missing set {:?}",
                            state.set
                        ),
                        span: None,
                        traceback: Vec::new(),
                    });
                }
            }
        }
    }
    if let Some(root_result) = &runtime.root_result {
        validate_runtime_value(runtime, root_result)?;
    }
    for request in &runtime.pending_host_calls {
        if runtime.promises.get(request.promise).is_none() {
            return Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message:
                    "snapshot validation failed: pending host call references a missing promise"
                        .to_string(),
                span: None,
                traceback: Vec::new(),
            });
        }
    }
    if let Some(request) = &runtime.suspended_host_call
        && runtime.promises.get(request.promise).is_none()
    {
        return Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: "snapshot validation failed: suspended host call references a missing promise"
                .to_string(),
            span: None,
            traceback: Vec::new(),
        });
    }
    for promise in runtime.promises.values() {
        match &promise.state {
            PromiseState::Pending => {}
            PromiseState::Fulfilled(value) => validate_runtime_value(runtime, value)?,
            PromiseState::Rejected(rejection) => validate_runtime_value(runtime, &rejection.value)?,
        }
        for reaction in &promise.reactions {
            match reaction {
                PromiseReaction::Then {
                    on_fulfilled,
                    on_rejected,
                    ..
                } => {
                    if let Some(handler) = on_fulfilled {
                        validate_runtime_value(runtime, handler)?;
                    }
                    if let Some(handler) = on_rejected {
                        validate_runtime_value(runtime, handler)?;
                    }
                }
                PromiseReaction::Finally { callback, .. } => {
                    if let Some(callback) = callback {
                        validate_runtime_value(runtime, callback)?;
                    }
                }
                PromiseReaction::FinallyPassThrough {
                    original_outcome, ..
                } => match original_outcome {
                    PromiseOutcome::Fulfilled(value) => validate_runtime_value(runtime, value)?,
                    PromiseOutcome::Rejected(rejection) => {
                        validate_runtime_value(runtime, &rejection.value)?
                    }
                },
                PromiseReaction::Combinator { .. } => {}
            }
        }
        if let Some(driver) = &promise.driver {
            match driver {
                PromiseDriver::All { values, .. } => {
                    for value in values.iter().flatten() {
                        validate_runtime_value(runtime, value)?;
                    }
                }
                PromiseDriver::AllSettled { results, .. } => {
                    for result in results.iter().flatten() {
                        match result {
                            PromiseSettledResult::Fulfilled(value)
                            | PromiseSettledResult::Rejected(value) => {
                                validate_runtime_value(runtime, value)?
                            }
                        }
                    }
                }
                PromiseDriver::Any { reasons, .. } => {
                    for value in reasons.iter().flatten() {
                        validate_runtime_value(runtime, value)?;
                    }
                }
            }
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
        Value::Map(map) if runtime.maps.get(*map).is_none() => Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: format!(
                "snapshot validation failed: value references missing map {:?}",
                map
            ),
            span: None,
            traceback: Vec::new(),
        }),
        Value::Set(set) if runtime.sets.get(*set).is_none() => Err(JsliteError::Message {
            kind: DiagnosticKind::Serialization,
            message: format!(
                "snapshot validation failed: value references missing set {:?}",
                set
            ),
            span: None,
            traceback: Vec::new(),
        }),
        Value::Iterator(iterator) if runtime.iterators.get(*iterator).is_none() => {
            Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: value references missing iterator {:?}",
                    iterator
                ),
                span: None,
                traceback: Vec::new(),
            })
        }
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
        Value::Promise(promise) if runtime.promises.get(*promise).is_none() => {
            Err(JsliteError::Message {
                kind: DiagnosticKind::Serialization,
                message: format!(
                    "snapshot validation failed: value references missing promise {:?}",
                    promise
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
    active_handlers: Vec<ActiveHandlerContext>,
    active_finally: Vec<ActiveFinallyContext>,
    finally_regions: Vec<FinallyRegionContext>,
    scope_depth: usize,
    internal_name_counter: usize,
}

#[derive(Debug, Default)]
struct LoopContext {
    break_jumps: Vec<ControlTransferPatch>,
    continue_jumps: Vec<ControlTransferPatch>,
    continue_target: Option<usize>,
    handler_depth: usize,
    scope_depth: usize,
}

#[derive(Debug, Clone, Copy)]
struct ActiveHandlerContext {
    finally_region: Option<usize>,
    scope_depth: usize,
}

#[derive(Debug, Default)]
struct FinallyRegionContext {
    handler_sites: Vec<usize>,
    jump_sites: Vec<usize>,
}

#[derive(Debug, Default)]
struct ActiveFinallyContext {
    exit_patch_site: usize,
    jump_sites: Vec<usize>,
    scope_depth: usize,
}

#[derive(Debug, Clone, Copy)]
enum ControlTransferPatch {
    DirectJump(usize),
    PendingJump(usize),
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
            rest: None,
            code: context.code,
            is_async: false,
            is_arrow: false,
            span,
        });
        Ok(id)
    }

    fn compile_function(&mut self, function: &FunctionExpr) -> JsliteResult<usize> {
        self.compile_function_body(function)
    }

    fn compile_function_body(&mut self, function: &FunctionExpr) -> JsliteResult<usize> {
        let mut context = CompileContext::default();
        self.emit_block_prologue(&mut context, &function.body)?;
        for statement in &function.body {
            self.compile_stmt(&mut context, statement)?;
        }
        context.code.push(Instruction::PushUndefined);
        context.code.push(Instruction::Return);
        let id = self.functions.len();
        self.functions.push(FunctionPrototype {
            name: function.name.clone(),
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

    fn fresh_internal_name(&self, context: &mut CompileContext, prefix: &str) -> String {
        let name = format!("\0jslite_{prefix}_{}", context.internal_name_counter);
        context.internal_name_counter += 1;
        name
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
                    handler_depth: context.active_handlers.len(),
                    scope_depth: context.scope_depth,
                    ..LoopContext::default()
                });
                self.compile_stmt(context, body)?;
                let loop_ctx = context.loop_stack.pop().unwrap_or_default();
                let continue_target = loop_ctx.continue_target.unwrap_or(loop_start);
                for jump in loop_ctx.continue_jumps {
                    self.patch_control_transfer(context, jump, continue_target);
                }
                context.code.push(Instruction::Jump(loop_start));
                let false_path_ip = context.code.len();
                context.code.push(Instruction::Pop);
                let loop_end = context.code.len();
                self.patch_jump(context, exit_jump, false_path_ip);
                for jump in loop_ctx.break_jumps {
                    self.patch_control_transfer(context, jump, loop_end);
                }
            }
            Stmt::DoWhile { body, test, .. } => {
                let loop_start = context.code.len();
                context.loop_stack.push(LoopContext {
                    handler_depth: context.active_handlers.len(),
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
                    self.patch_control_transfer(context, jump, continue_target);
                }
                for jump in loop_ctx.break_jumps {
                    self.patch_control_transfer(context, jump, loop_end);
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
                    handler_depth: context.active_handlers.len(),
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
                    self.patch_control_transfer(context, jump, update_start);
                }
                for jump in loop_ctx.break_jumps {
                    self.patch_control_transfer(context, jump, loop_end);
                }
                context.scope_depth -= 1;
                context.code.push(Instruction::PopEnv);
            }
            Stmt::ForOf {
                span,
                kind,
                pattern,
                iterable,
                body,
            } => {
                context.code.push(Instruction::PushEnv);
                context.scope_depth += 1;
                let loop_scope_depth = context.scope_depth;
                let iterator_binding = self.fresh_internal_name(context, "iter");
                context.code.push(Instruction::DeclareName {
                    name: iterator_binding.clone(),
                    mutable: false,
                });
                self.compile_expr(context, iterable)?;
                context.code.push(Instruction::CreateIterator);
                context
                    .code
                    .push(Instruction::InitializePattern(Pattern::Identifier {
                        span: *span,
                        name: iterator_binding.clone(),
                    }));

                let loop_start = context.code.len();
                context
                    .code
                    .push(Instruction::LoadName(iterator_binding.clone()));
                context.code.push(Instruction::IteratorNext);
                let exit_jump = self.emit_jump(context, Instruction::JumpIfTrue(usize::MAX));
                context.code.push(Instruction::Pop);

                context.code.push(Instruction::PushEnv);
                context.scope_depth += 1;
                for (name, _) in pattern_bindings(pattern) {
                    context.code.push(Instruction::DeclareName {
                        name,
                        mutable: *kind == BindingKind::Let,
                    });
                }
                context
                    .code
                    .push(Instruction::InitializePattern(pattern.clone()));
                context.loop_stack.push(LoopContext {
                    handler_depth: context.active_handlers.len(),
                    scope_depth: loop_scope_depth,
                    ..LoopContext::default()
                });
                self.compile_stmt(context, body)?;
                context.scope_depth -= 1;
                context.code.push(Instruction::PopEnv);
                let continue_target = context.code.len();
                context.code.push(Instruction::Jump(loop_start));

                let done_path_ip = context.code.len();
                context.code.push(Instruction::Pop);
                context.code.push(Instruction::Pop);
                let loop_end = context.code.len();
                self.patch_jump(context, exit_jump, done_path_ip);

                let loop_ctx = context.loop_stack.pop().unwrap_or_default();
                for jump in loop_ctx.continue_jumps {
                    self.patch_control_transfer(context, jump, continue_target);
                }
                for jump in loop_ctx.break_jumps {
                    self.patch_control_transfer(context, jump, loop_end);
                }

                context.scope_depth -= 1;
                context.code.push(Instruction::PopEnv);
            }
            Stmt::Break { span } => {
                let Some(loop_ctx) = context.loop_stack.last() else {
                    return Err(JsliteError::runtime_at(
                        "`break` used outside of a loop",
                        *span,
                    ));
                };
                let patch =
                    self.emit_jump_transfer(context, loop_ctx.handler_depth, loop_ctx.scope_depth);
                context
                    .loop_stack
                    .last_mut()
                    .expect("loop context should still exist")
                    .break_jumps
                    .push(patch);
            }
            Stmt::Continue { span } => {
                let Some(loop_ctx) = context.loop_stack.last() else {
                    return Err(JsliteError::runtime_at(
                        "`continue` used outside of a loop",
                        *span,
                    ));
                };
                let patch =
                    self.emit_jump_transfer(context, loop_ctx.handler_depth, loop_ctx.scope_depth);
                context
                    .loop_stack
                    .last_mut()
                    .expect("loop context should still exist")
                    .continue_jumps
                    .push(patch);
            }
            Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    self.compile_expr(context, value)?;
                } else {
                    context.code.push(Instruction::PushUndefined);
                }
                self.emit_return(context);
            }
            Stmt::Throw { span, value } => {
                self.compile_expr(context, value)?;
                if let Some(active_finally) = context.active_finally.last() {
                    self.emit_scope_cleanup(context, active_finally.scope_depth);
                    context.code.push(Instruction::PushPendingThrow);
                    self.emit_jump_to_active_finally_exit(context);
                } else {
                    context.code.push(Instruction::Throw { span: *span });
                }
            }
            Stmt::Try {
                body,
                catch,
                finally,
                ..
            } => {
                self.compile_try(context, body, catch.as_ref(), finally.as_deref())?;
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
                    handler_depth: context.active_handlers.len(),
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
                    self.patch_control_transfer(context, jump, end_ip);
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
            Expr::Await { value, .. } => {
                self.compile_expr(context, value)?;
                context.code.push(Instruction::Await);
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
                } else if operator == AssignOp::NullishAssign {
                    context.code.push(Instruction::LoadName(name.clone()));
                    context.code.push(Instruction::Dup);
                    let rhs_jump = self.emit_jump(context, Instruction::JumpIfNullish(usize::MAX));
                    context.code.push(Instruction::Pop);
                    let end_jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
                    let rhs_ip = context.code.len();
                    self.patch_jump(context, rhs_jump, rhs_ip);
                    context.code.push(Instruction::Pop);
                    context.code.push(Instruction::Pop);
                    self.compile_expr(context, value)?;
                    context.code.push(Instruction::StoreName(name.clone()));
                    let end_ip = context.code.len();
                    self.patch_jump(context, end_jump, end_ip);
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
                    if operator == AssignOp::Assign {
                        self.compile_expr(context, object)?;
                        self.compile_expr(context, value)?;
                        context
                            .code
                            .push(Instruction::SetPropStatic { name: name.clone() });
                    } else if operator == AssignOp::NullishAssign {
                        let object_binding = self.fresh_internal_name(context, "assign_obj");
                        context.code.push(Instruction::DeclareName {
                            name: object_binding.clone(),
                            mutable: false,
                        });
                        self.compile_expr(context, object)?;
                        context
                            .code
                            .push(Instruction::InitializePattern(Pattern::Identifier {
                                span: SourceSpan::new(0, 0),
                                name: object_binding.clone(),
                            }));
                        context
                            .code
                            .push(Instruction::LoadName(object_binding.clone()));
                        context.code.push(Instruction::GetPropStatic {
                            name: name.clone(),
                            optional: *optional,
                        });
                        context.code.push(Instruction::Dup);
                        let rhs_jump =
                            self.emit_jump(context, Instruction::JumpIfNullish(usize::MAX));
                        context.code.push(Instruction::Pop);
                        let end_jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
                        let rhs_ip = context.code.len();
                        self.patch_jump(context, rhs_jump, rhs_ip);
                        context.code.push(Instruction::Pop);
                        context.code.push(Instruction::Pop);
                        context.code.push(Instruction::LoadName(object_binding));
                        self.compile_expr(context, value)?;
                        context
                            .code
                            .push(Instruction::SetPropStatic { name: name.clone() });
                        let end_ip = context.code.len();
                        self.patch_jump(context, end_jump, end_ip);
                    } else {
                        self.compile_expr(context, object)?;
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
                    let name = format_number_key(*number);
                    if operator == AssignOp::Assign {
                        self.compile_expr(context, object)?;
                        self.compile_expr(context, value)?;
                        context.code.push(Instruction::SetPropStatic { name });
                    } else if operator == AssignOp::NullishAssign {
                        let object_binding = self.fresh_internal_name(context, "assign_obj");
                        context.code.push(Instruction::DeclareName {
                            name: object_binding.clone(),
                            mutable: false,
                        });
                        self.compile_expr(context, object)?;
                        context
                            .code
                            .push(Instruction::InitializePattern(Pattern::Identifier {
                                span: SourceSpan::new(0, 0),
                                name: object_binding.clone(),
                            }));
                        context
                            .code
                            .push(Instruction::LoadName(object_binding.clone()));
                        context.code.push(Instruction::GetPropStatic {
                            name: name.clone(),
                            optional: *optional,
                        });
                        context.code.push(Instruction::Dup);
                        let rhs_jump =
                            self.emit_jump(context, Instruction::JumpIfNullish(usize::MAX));
                        context.code.push(Instruction::Pop);
                        let end_jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
                        let rhs_ip = context.code.len();
                        self.patch_jump(context, rhs_jump, rhs_ip);
                        context.code.push(Instruction::Pop);
                        context.code.push(Instruction::Pop);
                        context.code.push(Instruction::LoadName(object_binding));
                        self.compile_expr(context, value)?;
                        context.code.push(Instruction::SetPropStatic { name });
                        let end_ip = context.code.len();
                        self.patch_jump(context, end_jump, end_ip);
                    } else {
                        self.compile_expr(context, object)?;
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
                    if operator == AssignOp::Assign {
                        self.compile_expr(context, object)?;
                        self.compile_expr(context, expr)?;
                        self.compile_expr(context, value)?;
                        context.code.push(Instruction::SetPropComputed);
                    } else if operator == AssignOp::NullishAssign {
                        let object_binding = self.fresh_internal_name(context, "assign_obj");
                        let key_binding = self.fresh_internal_name(context, "assign_key");
                        for name in [&object_binding, &key_binding] {
                            context.code.push(Instruction::DeclareName {
                                name: name.clone(),
                                mutable: false,
                            });
                        }
                        self.compile_expr(context, object)?;
                        context
                            .code
                            .push(Instruction::InitializePattern(Pattern::Identifier {
                                span: SourceSpan::new(0, 0),
                                name: object_binding.clone(),
                            }));
                        self.compile_expr(context, expr)?;
                        context
                            .code
                            .push(Instruction::InitializePattern(Pattern::Identifier {
                                span: SourceSpan::new(0, 0),
                                name: key_binding.clone(),
                            }));
                        context
                            .code
                            .push(Instruction::LoadName(object_binding.clone()));
                        context
                            .code
                            .push(Instruction::LoadName(key_binding.clone()));
                        context.code.push(Instruction::GetPropComputed {
                            optional: *optional,
                        });
                        context.code.push(Instruction::Dup);
                        let rhs_jump =
                            self.emit_jump(context, Instruction::JumpIfNullish(usize::MAX));
                        context.code.push(Instruction::Pop);
                        let end_jump = self.emit_jump(context, Instruction::Jump(usize::MAX));
                        let rhs_ip = context.code.len();
                        self.patch_jump(context, rhs_jump, rhs_ip);
                        context.code.push(Instruction::Pop);
                        context.code.push(Instruction::Pop);
                        context.code.push(Instruction::LoadName(object_binding));
                        context.code.push(Instruction::LoadName(key_binding));
                        self.compile_expr(context, value)?;
                        context.code.push(Instruction::SetPropComputed);
                        let end_ip = context.code.len();
                        self.patch_jump(context, end_jump, end_ip);
                    } else {
                        self.compile_expr(context, object)?;
                        self.compile_expr(context, expr)?;
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

    fn compile_try(
        &mut self,
        context: &mut CompileContext,
        body: &Stmt,
        catch: Option<&crate::ir::CatchClause>,
        finally: Option<&Stmt>,
    ) -> JsliteResult<()> {
        let finally_region = finally.map(|_| {
            context
                .finally_regions
                .push(FinallyRegionContext::default());
            context.finally_regions.len() - 1
        });

        let try_handler_site = context.code.len();
        context.code.push(Instruction::PushHandler {
            catch: catch.map(|_| usize::MAX),
            finally: finally_region.map(|_| usize::MAX),
        });
        if let Some(region) = finally_region {
            context.finally_regions[region]
                .handler_sites
                .push(try_handler_site);
        }

        context.active_handlers.push(ActiveHandlerContext {
            finally_region,
            scope_depth: context.scope_depth,
        });
        self.compile_stmt(context, body)?;
        context.active_handlers.pop();
        context.code.push(Instruction::PopHandler);

        let mut skip_catch_jump = None;
        let mut after_finally_patches = Vec::new();
        let outer_handler_depth = context.active_handlers.len();

        if let Some(region) = finally_region {
            let patch = context.code.len();
            context.code.push(Instruction::PushPendingJump {
                target: usize::MAX,
                target_handler_depth: outer_handler_depth,
                target_scope_depth: context.scope_depth,
            });
            after_finally_patches.push(patch);
            self.emit_jump_to_finally(context, region);
        } else if catch.is_some() {
            skip_catch_jump = Some(self.emit_jump(context, Instruction::Jump(usize::MAX)));
        }

        if let Some(catch_clause) = catch {
            self.patch_handler_catch(context, try_handler_site, context.code.len());

            if let Some(region) = finally_region {
                let catch_handler_site = context.code.len();
                context.code.push(Instruction::PushHandler {
                    catch: None,
                    finally: Some(usize::MAX),
                });
                context.finally_regions[region]
                    .handler_sites
                    .push(catch_handler_site);
                context.active_handlers.push(ActiveHandlerContext {
                    finally_region: Some(region),
                    scope_depth: context.scope_depth,
                });
            }

            context.code.push(Instruction::PushEnv);
            context.scope_depth += 1;
            if let Some(parameter) = &catch_clause.parameter {
                for (name, mutable) in pattern_bindings(parameter) {
                    context
                        .code
                        .push(Instruction::DeclareName { name, mutable });
                }
            }
            context.code.push(Instruction::BeginCatch);
            if let Some(parameter) = &catch_clause.parameter {
                context
                    .code
                    .push(Instruction::InitializePattern(parameter.clone()));
            } else {
                context.code.push(Instruction::Pop);
            }
            self.compile_stmt(context, catch_clause.body.as_ref())?;
            context.scope_depth -= 1;
            context.code.push(Instruction::PopEnv);

            if let Some(region) = finally_region {
                context.active_handlers.pop();
                context.code.push(Instruction::PopHandler);
                let patch = context.code.len();
                context.code.push(Instruction::PushPendingJump {
                    target: usize::MAX,
                    target_handler_depth: outer_handler_depth,
                    target_scope_depth: context.scope_depth,
                });
                after_finally_patches.push(patch);
                self.emit_jump_to_finally(context, region);
            }
        }

        if let Some(finally_stmt) = finally {
            let finally_ip = context.code.len();
            self.patch_finally_region(
                context,
                finally_region.expect("finally region should exist"),
                finally_ip,
            );
            let enter_finally = context.code.len();
            context
                .code
                .push(Instruction::EnterFinally { exit: usize::MAX });
            context.active_finally.push(ActiveFinallyContext {
                exit_patch_site: enter_finally,
                jump_sites: Vec::new(),
                scope_depth: context.scope_depth,
            });
            self.compile_stmt(context, finally_stmt)?;
            let continue_ip = context.code.len();
            let active_finally = context
                .active_finally
                .pop()
                .expect("finally context should exist");
            self.patch_finally_exit(context, active_finally, continue_ip);
            context.code.push(Instruction::ContinuePending);
            let after_finally = context.code.len();
            for patch in after_finally_patches {
                self.patch_pending_jump(context, patch, after_finally);
            }
            if let Some(skip_catch_jump) = skip_catch_jump {
                self.patch_jump(context, skip_catch_jump, after_finally);
            }
        } else if let Some(skip_catch_jump) = skip_catch_jump {
            let after_catch = context.code.len();
            self.patch_jump(context, skip_catch_jump, after_catch);
        }

        Ok(())
    }

    fn emit_return(&self, context: &mut CompileContext) {
        if let Some(active_finally) = context.active_finally.last() {
            self.emit_scope_cleanup(context, active_finally.scope_depth);
            context.code.push(Instruction::PushPendingReturn);
            self.emit_jump_to_active_finally_exit(context);
            return;
        }
        if let Some((handler_depth, region)) = self.nearest_finally_region(context, 0) {
            self.emit_scope_cleanup(context, context.active_handlers[handler_depth].scope_depth);
            self.emit_handler_cleanup(context, handler_depth);
            context.code.push(Instruction::PushPendingReturn);
            self.emit_jump_to_finally(context, region);
        } else {
            context.code.push(Instruction::Return);
        }
    }

    fn emit_jump_transfer(
        &self,
        context: &mut CompileContext,
        target_handler_depth: usize,
        target_scope_depth: usize,
    ) -> ControlTransferPatch {
        if let Some(active_finally) = context.active_finally.last() {
            self.emit_scope_cleanup(context, active_finally.scope_depth);
            let patch = context.code.len();
            context.code.push(Instruction::PushPendingJump {
                target: usize::MAX,
                target_handler_depth,
                target_scope_depth,
            });
            self.emit_jump_to_active_finally_exit(context);
            return ControlTransferPatch::PendingJump(patch);
        }
        if let Some((handler_depth, region)) =
            self.nearest_finally_region(context, target_handler_depth)
        {
            self.emit_scope_cleanup(context, context.active_handlers[handler_depth].scope_depth);
            self.emit_handler_cleanup(context, handler_depth);
            let patch = context.code.len();
            context.code.push(Instruction::PushPendingJump {
                target: usize::MAX,
                target_handler_depth,
                target_scope_depth,
            });
            self.emit_jump_to_finally(context, region);
            ControlTransferPatch::PendingJump(patch)
        } else {
            self.emit_scope_cleanup(context, target_scope_depth);
            self.emit_handler_cleanup(context, target_handler_depth);
            ControlTransferPatch::DirectJump(self.emit_jump(context, Instruction::Jump(usize::MAX)))
        }
    }

    fn emit_scope_cleanup(&self, context: &mut CompileContext, target_scope_depth: usize) {
        for _ in target_scope_depth..context.scope_depth {
            context.code.push(Instruction::PopEnv);
        }
    }

    fn emit_handler_cleanup(&self, context: &mut CompileContext, target_handler_depth: usize) {
        for _ in target_handler_depth..context.active_handlers.len() {
            context.code.push(Instruction::PopHandler);
        }
    }

    fn nearest_finally_region(
        &self,
        context: &CompileContext,
        target_handler_depth: usize,
    ) -> Option<(usize, usize)> {
        for handler_depth in (target_handler_depth..context.active_handlers.len()).rev() {
            if let Some(region) = context.active_handlers[handler_depth].finally_region {
                return Some((handler_depth, region));
            }
        }
        None
    }

    fn emit_jump_to_finally(&self, context: &mut CompileContext, region: usize) {
        let jump_site = self.emit_jump(context, Instruction::Jump(usize::MAX));
        context.finally_regions[region].jump_sites.push(jump_site);
    }

    fn emit_jump_to_active_finally_exit(&self, context: &mut CompileContext) {
        let jump_site = self.emit_jump(context, Instruction::Jump(usize::MAX));
        context
            .active_finally
            .last_mut()
            .expect("finally context should exist")
            .jump_sites
            .push(jump_site);
    }

    fn patch_handler_catch(&self, context: &mut CompileContext, index: usize, target: usize) {
        if let Instruction::PushHandler { catch, .. } = &mut context.code[index] {
            *catch = Some(target);
        }
    }

    fn patch_finally_region(&self, context: &mut CompileContext, region: usize, target: usize) {
        let handler_sites = context.finally_regions[region].handler_sites.clone();
        let jump_sites = context.finally_regions[region].jump_sites.clone();
        for site in handler_sites {
            if let Instruction::PushHandler { finally, .. } = &mut context.code[site] {
                *finally = Some(target);
            }
        }
        for site in jump_sites {
            self.patch_jump(context, site, target);
        }
    }

    fn patch_finally_exit(
        &self,
        context: &mut CompileContext,
        finally: ActiveFinallyContext,
        target: usize,
    ) {
        if let Instruction::EnterFinally { exit } = &mut context.code[finally.exit_patch_site] {
            *exit = target;
        }
        for jump_site in finally.jump_sites {
            self.patch_jump(context, jump_site, target);
        }
    }

    fn patch_pending_jump(&self, context: &mut CompileContext, index: usize, target: usize) {
        if let Instruction::PushPendingJump { target: jump, .. } = &mut context.code[index] {
            *jump = target;
        }
    }

    fn patch_control_transfer(
        &self,
        context: &mut CompileContext,
        patch: ControlTransferPatch,
        target: usize,
    ) {
        match patch {
            ControlTransferPatch::DirectJump(index) => self.patch_jump(context, index, target),
            ControlTransferPatch::PendingJump(index) => {
                self.patch_pending_jump(context, index, target)
            }
        }
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
        AssignOp::Assign | AssignOp::NullishAssign => {
            Err(JsliteError::runtime("invalid compound assignment"))
        }
        AssignOp::AddAssign => Ok(BinaryOp::Add),
        AssignOp::SubAssign => Ok(BinaryOp::Sub),
        AssignOp::MulAssign => Ok(BinaryOp::Mul),
        AssignOp::DivAssign => Ok(BinaryOp::Div),
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
    Map(MapKey),
    Set(SetKey),
    Iterator(IteratorKey),
    Closure(ClosureKey),
    Promise(PromiseKey),
    BuiltinFunction(BuiltinFunction),
    HostFunction(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum BuiltinFunction {
    ArrayCtor,
    ArrayIsArray,
    ArrayPush,
    ArrayPop,
    ArraySlice,
    ArrayJoin,
    ArrayIncludes,
    ArrayIndexOf,
    ArrayValues,
    ArrayKeys,
    ArrayEntries,
    ObjectCtor,
    ObjectKeys,
    ObjectValues,
    ObjectEntries,
    ObjectHasOwn,
    MapCtor,
    MapGet,
    MapSet,
    MapHas,
    MapDelete,
    MapClear,
    MapEntries,
    MapKeys,
    MapValues,
    SetCtor,
    SetAdd,
    SetHas,
    SetDelete,
    SetClear,
    SetEntries,
    SetKeys,
    SetValues,
    IteratorNext,
    PromiseCtor,
    PromiseResolve,
    PromiseReject,
    PromiseThen,
    PromiseCatch,
    PromiseFinally,
    PromiseAll,
    PromiseRace,
    PromiseAny,
    PromiseAllSettled,
    ErrorCtor,
    TypeErrorCtor,
    ReferenceErrorCtor,
    RangeErrorCtor,
    NumberCtor,
    StringCtor,
    StringTrim,
    StringIncludes,
    StringStartsWith,
    StringEndsWith,
    StringSlice,
    StringSubstring,
    StringToLowerCase,
    StringToUpperCase,
    BooleanCtor,
    MathAbs,
    MathMax,
    MathMin,
    MathFloor,
    MathCeil,
    MathRound,
    MathPow,
    MathSqrt,
    MathTrunc,
    MathSign,
    JsonStringify,
    JsonParse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Env {
    parent: Option<EnvKey>,
    bindings: IndexMap<String, CellKey>,
    #[serde(skip, default)]
    accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Cell {
    value: Value,
    mutable: bool,
    initialized: bool,
    #[serde(skip, default)]
    accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlainObject {
    properties: IndexMap<String, Value>,
    kind: ObjectKind,
    #[serde(skip, default)]
    accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ObjectKind {
    Plain,
    Global,
    Math,
    Json,
    Console,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArrayObject {
    elements: Vec<Value>,
    properties: IndexMap<String, Value>,
    #[serde(skip, default)]
    accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MapObject {
    entries: Vec<MapEntry>,
    #[serde(skip, default)]
    accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MapEntry {
    key: Value,
    value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetObject {
    entries: Vec<Value>,
    #[serde(skip, default)]
    accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IteratorObject {
    state: IteratorState,
    #[serde(skip, default)]
    accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum IteratorState {
    Array(ArrayIteratorState),
    ArrayKeys(ArrayIteratorState),
    ArrayEntries(ArrayIteratorState),
    String(StringIteratorState),
    MapEntries(MapIteratorState),
    MapKeys(MapIteratorState),
    MapValues(MapIteratorState),
    SetEntries(SetIteratorState),
    SetValues(SetIteratorState),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArrayIteratorState {
    array: ArrayKey,
    next_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StringIteratorState {
    value: String,
    next_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MapIteratorState {
    map: MapKey,
    next_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetIteratorState {
    set: SetKey,
    next_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Closure {
    function_id: usize,
    env: EnvKey,
    #[serde(skip, default)]
    accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PromiseObject {
    state: PromiseState,
    awaiters: Vec<AsyncContinuation>,
    dependents: Vec<PromiseKey>,
    reactions: Vec<PromiseReaction>,
    driver: Option<PromiseDriver>,
    #[serde(skip, default)]
    accounted_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum PromiseState {
    Pending,
    Fulfilled(Value),
    Rejected(PromiseRejection),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PromiseRejection {
    value: Value,
    span: Option<SourceSpan>,
    traceback: Vec<TraceFrameSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TraceFrameSnapshot {
    function_name: Option<String>,
    span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AsyncContinuation {
    frames: Vec<Frame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum PromiseOutcome {
    Fulfilled(Value),
    Rejected(PromiseRejection),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum PromiseReaction {
    Then {
        target: PromiseKey,
        on_fulfilled: Option<Value>,
        on_rejected: Option<Value>,
    },
    Finally {
        target: PromiseKey,
        callback: Option<Value>,
    },
    FinallyPassThrough {
        target: PromiseKey,
        original_outcome: PromiseOutcome,
    },
    Combinator {
        target: PromiseKey,
        index: usize,
        kind: PromiseCombinatorKind,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum PromiseCombinatorKind {
    All,
    AllSettled,
    Any,
    Race,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum PromiseDriver {
    All {
        remaining: usize,
        values: Vec<Option<Value>>,
    },
    AllSettled {
        remaining: usize,
        results: Vec<Option<PromiseSettledResult>>,
    },
    Any {
        remaining: usize,
        reasons: Vec<Option<Value>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum PromiseSettledResult {
    Fulfilled(Value),
    Rejected(Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum MicrotaskJob {
    ResumeAsync {
        continuation: AsyncContinuation,
        outcome: PromiseOutcome,
    },
    PromiseReaction {
        reaction: PromiseReaction,
        outcome: PromiseOutcome,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingHostCall {
    capability: String,
    args: Vec<StructuredValue>,
    promise: PromiseKey,
    resume_behavior: ResumeBehavior,
    traceback: Vec<TraceFrameSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Frame {
    function_id: usize,
    ip: usize,
    env: EnvKey,
    scope_stack: Vec<EnvKey>,
    stack: Vec<Value>,
    handlers: Vec<ExceptionHandler>,
    pending_exception: Option<Value>,
    pending_completions: Vec<CompletionRecord>,
    active_finally: Vec<ActiveFinallyState>,
    async_promise: Option<PromiseKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExceptionHandler {
    catch: Option<usize>,
    finally: Option<usize>,
    env: EnvKey,
    scope_stack_len: usize,
    stack_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CompletionRecord {
    Jump {
        target: usize,
        target_handler_depth: usize,
        target_scope_depth: usize,
    },
    Return(Value),
    Throw(Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActiveFinallyState {
    completion_index: usize,
    exit: usize,
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
    maps: SlotMap<MapKey, MapObject>,
    sets: SlotMap<SetKey, SetObject>,
    iterators: SlotMap<IteratorKey, IteratorObject>,
    closures: SlotMap<ClosureKey, Closure>,
    promises: SlotMap<PromiseKey, PromiseObject>,
    frames: Vec<Frame>,
    root_result: Option<Value>,
    microtasks: VecDeque<MicrotaskJob>,
    pending_host_calls: VecDeque<PendingHostCall>,
    suspended_host_call: Option<PendingHostCall>,
    instruction_counter: usize,
    #[serde(skip, default)]
    heap_bytes_used: usize,
    #[serde(skip, default)]
    allocation_count: usize,
    #[serde(skip, default)]
    cancellation_token: Option<CancellationToken>,
    pending_resume_behavior: ResumeBehavior,
}

enum RunState {
    Completed(Value),
    PushedFrame,
    StartedAsync(Value),
    Suspended {
        capability: String,
        args: Vec<StructuredValue>,
        resume_behavior: ResumeBehavior,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum ResumeBehavior {
    Value,
    Undefined,
}

enum StepAction {
    Continue,
    Return(ExecutionStep),
}

#[derive(Debug, Default)]
struct GarbageCollectionMarks {
    envs: HashSet<EnvKey>,
    cells: HashSet<CellKey>,
    objects: HashSet<ObjectKey>,
    arrays: HashSet<ArrayKey>,
    maps: HashSet<MapKey>,
    sets: HashSet<SetKey>,
    iterators: HashSet<IteratorKey>,
    closures: HashSet<ClosureKey>,
    promises: HashSet<PromiseKey>,
}

#[derive(Debug, Default)]
struct GarbageCollectionWorklist {
    envs: Vec<EnvKey>,
    cells: Vec<CellKey>,
    objects: Vec<ObjectKey>,
    arrays: Vec<ArrayKey>,
    maps: Vec<MapKey>,
    sets: Vec<SetKey>,
    iterators: Vec<IteratorKey>,
    closures: Vec<ClosureKey>,
    promises: Vec<PromiseKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GarbageCollectionStats {
    reclaimed_bytes: usize,
    reclaimed_allocations: usize,
}

impl Runtime {
    fn new(program: BytecodeProgram, options: ExecutionOptions) -> JsliteResult<Self> {
        let ExecutionOptions {
            inputs,
            capabilities,
            limits,
            cancellation_token,
        } = options;
        let mut envs = SlotMap::with_key();
        let globals = envs.insert(Env {
            parent: None,
            bindings: IndexMap::new(),
            accounted_bytes: 0,
        });
        let mut runtime = Self {
            program,
            limits,
            globals,
            envs,
            cells: SlotMap::with_key(),
            objects: SlotMap::with_key(),
            arrays: SlotMap::with_key(),
            maps: SlotMap::with_key(),
            sets: SlotMap::with_key(),
            iterators: SlotMap::with_key(),
            closures: SlotMap::with_key(),
            promises: SlotMap::with_key(),
            frames: Vec::new(),
            root_result: None,
            microtasks: VecDeque::new(),
            pending_host_calls: VecDeque::new(),
            suspended_host_call: None,
            instruction_counter: 0,
            heap_bytes_used: 0,
            allocation_count: 0,
            cancellation_token,
            pending_resume_behavior: ResumeBehavior::Value,
        };
        runtime.account_existing_env(globals)?;
        runtime.install_builtins()?;
        for capability in capabilities {
            runtime.define_global(capability.clone(), Value::HostFunction(capability), false)?;
        }
        for (name, value) in inputs {
            let value = runtime.value_from_structured(value)?;
            runtime.define_global(name, value, true)?;
        }
        Ok(runtime)
    }

    fn apply_resume_options(&mut self, options: ResumeOptions) {
        if options.cancellation_token.is_some() {
            self.cancellation_token = options.cancellation_token;
        }
    }

    fn check_cancellation(&self) -> JsliteResult<()> {
        if self
            .cancellation_token
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            return Err(limit_error("execution cancelled"));
        }
        Ok(())
    }

    fn run_root(&mut self) -> JsliteResult<ExecutionStep> {
        self.check_cancellation()?;
        self.collect_garbage()?;
        self.check_call_depth()?;
        let root_env = self.new_env(Some(self.globals))?;
        self.push_frame(self.program.root, root_env, &[], Value::Undefined, None)?;
        self.run()
    }

    fn ensure_heap_capacity(&self, additional_bytes: usize) -> JsliteResult<()> {
        let next = self
            .heap_bytes_used
            .checked_add(additional_bytes)
            .ok_or_else(|| limit_error("heap limit exceeded"))?;
        if next > self.limits.heap_limit_bytes {
            return Err(limit_error("heap limit exceeded"));
        }
        Ok(())
    }

    fn account_new_allocation(&mut self, bytes: usize) -> JsliteResult<()> {
        let next_allocations = self
            .allocation_count
            .checked_add(1)
            .ok_or_else(|| limit_error("allocation budget exhausted"))?;
        if next_allocations > self.limits.allocation_budget {
            return Err(limit_error("allocation budget exhausted"));
        }
        self.ensure_heap_capacity(bytes)?;
        self.allocation_count = next_allocations;
        self.heap_bytes_used += bytes;
        Ok(())
    }

    fn apply_heap_delta(&mut self, old_bytes: usize, new_bytes: usize) -> JsliteResult<()> {
        if new_bytes >= old_bytes {
            self.ensure_heap_capacity(new_bytes - old_bytes)?;
            self.heap_bytes_used += new_bytes - old_bytes;
        } else {
            self.heap_bytes_used -= old_bytes - new_bytes;
        }
        Ok(())
    }

    fn insert_env(&mut self, parent: Option<EnvKey>) -> JsliteResult<EnvKey> {
        let mut env = Env {
            parent,
            bindings: IndexMap::new(),
            accounted_bytes: 0,
        };
        env.accounted_bytes = measure_env_bytes(&env);
        self.account_new_allocation(env.accounted_bytes)?;
        Ok(self.envs.insert(env))
    }

    fn insert_cell(
        &mut self,
        value: Value,
        mutable: bool,
        initialized: bool,
    ) -> JsliteResult<CellKey> {
        let mut cell = Cell {
            value,
            mutable,
            initialized,
            accounted_bytes: 0,
        };
        cell.accounted_bytes = measure_cell_bytes(&cell);
        self.account_new_allocation(cell.accounted_bytes)?;
        Ok(self.cells.insert(cell))
    }

    fn insert_object(
        &mut self,
        properties: IndexMap<String, Value>,
        kind: ObjectKind,
    ) -> JsliteResult<ObjectKey> {
        let mut object = PlainObject {
            properties,
            kind,
            accounted_bytes: 0,
        };
        object.accounted_bytes = measure_object_bytes(&object);
        self.account_new_allocation(object.accounted_bytes)?;
        Ok(self.objects.insert(object))
    }

    fn insert_array(
        &mut self,
        elements: Vec<Value>,
        properties: IndexMap<String, Value>,
    ) -> JsliteResult<ArrayKey> {
        let mut array = ArrayObject {
            elements,
            properties,
            accounted_bytes: 0,
        };
        array.accounted_bytes = measure_array_bytes(&array);
        self.account_new_allocation(array.accounted_bytes)?;
        Ok(self.arrays.insert(array))
    }

    fn insert_map(&mut self, entries: Vec<MapEntry>) -> JsliteResult<MapKey> {
        let mut map = MapObject {
            entries,
            accounted_bytes: 0,
        };
        map.accounted_bytes = measure_map_bytes(&map);
        self.account_new_allocation(map.accounted_bytes)?;
        Ok(self.maps.insert(map))
    }

    fn insert_set(&mut self, entries: Vec<Value>) -> JsliteResult<SetKey> {
        let mut set = SetObject {
            entries,
            accounted_bytes: 0,
        };
        set.accounted_bytes = measure_set_bytes(&set);
        self.account_new_allocation(set.accounted_bytes)?;
        Ok(self.sets.insert(set))
    }

    fn insert_iterator(&mut self, state: IteratorState) -> JsliteResult<IteratorKey> {
        let mut iterator = IteratorObject {
            state,
            accounted_bytes: 0,
        };
        iterator.accounted_bytes = measure_iterator_bytes(&iterator);
        self.account_new_allocation(iterator.accounted_bytes)?;
        Ok(self.iterators.insert(iterator))
    }

    fn insert_closure(&mut self, function_id: usize, env: EnvKey) -> JsliteResult<ClosureKey> {
        let mut closure = Closure {
            function_id,
            env,
            accounted_bytes: 0,
        };
        closure.accounted_bytes = measure_closure_bytes(&closure);
        self.account_new_allocation(closure.accounted_bytes)?;
        Ok(self.closures.insert(closure))
    }

    fn insert_promise(&mut self, state: PromiseState) -> JsliteResult<PromiseKey> {
        let mut promise = PromiseObject {
            state,
            awaiters: Vec::new(),
            dependents: Vec::new(),
            reactions: Vec::new(),
            driver: None,
            accounted_bytes: 0,
        };
        promise.accounted_bytes = measure_promise_bytes(&promise);
        self.account_new_allocation(promise.accounted_bytes)?;
        Ok(self.promises.insert(promise))
    }

    fn account_existing_env(&mut self, key: EnvKey) -> JsliteResult<()> {
        let bytes = {
            let env = self
                .envs
                .get(key)
                .ok_or_else(|| JsliteError::runtime("environment missing"))?;
            measure_env_bytes(env)
        };
        self.account_new_allocation(bytes)?;
        self.envs
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("environment missing"))?
            .accounted_bytes = bytes;
        Ok(())
    }

    fn refresh_env_accounting(&mut self, key: EnvKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let env = self
                .envs
                .get(key)
                .ok_or_else(|| JsliteError::runtime("environment missing"))?;
            (env.accounted_bytes, measure_env_bytes(env))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.envs
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("environment missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    fn refresh_cell_accounting(&mut self, key: CellKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let cell = self
                .cells
                .get(key)
                .ok_or_else(|| JsliteError::runtime("binding cell missing"))?;
            (cell.accounted_bytes, measure_cell_bytes(cell))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.cells
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("binding cell missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    fn refresh_object_accounting(&mut self, key: ObjectKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let object = self
                .objects
                .get(key)
                .ok_or_else(|| JsliteError::runtime("object missing"))?;
            (object.accounted_bytes, measure_object_bytes(object))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.objects
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("object missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    fn refresh_array_accounting(&mut self, key: ArrayKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let array = self
                .arrays
                .get(key)
                .ok_or_else(|| JsliteError::runtime("array missing"))?;
            (array.accounted_bytes, measure_array_bytes(array))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.arrays
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    fn refresh_map_accounting(&mut self, key: MapKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let map = self
                .maps
                .get(key)
                .ok_or_else(|| JsliteError::runtime("map missing"))?;
            (map.accounted_bytes, measure_map_bytes(map))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.maps
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("map missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    fn refresh_set_accounting(&mut self, key: SetKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let set = self
                .sets
                .get(key)
                .ok_or_else(|| JsliteError::runtime("set missing"))?;
            (set.accounted_bytes, measure_set_bytes(set))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.sets
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("set missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    fn refresh_iterator_accounting(&mut self, key: IteratorKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let iterator = self
                .iterators
                .get(key)
                .ok_or_else(|| JsliteError::runtime("iterator missing"))?;
            (iterator.accounted_bytes, measure_iterator_bytes(iterator))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.iterators
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("iterator missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    fn recompute_accounting_after_load(&mut self) -> JsliteResult<()> {
        let (heap_bytes_used, allocation_count) =
            self.recompute_accounting_totals().map_err(|message| {
                serialization_error(format!("snapshot validation failed: {message}"))
            })?;

        if heap_bytes_used > self.limits.heap_limit_bytes {
            return Err(serialization_error(
                "snapshot validation failed: heap usage exceeds configured heap limit",
            ));
        }
        if allocation_count > self.limits.allocation_budget {
            return Err(serialization_error(
                "snapshot validation failed: allocation count exceeds configured allocation budget",
            ));
        }

        self.heap_bytes_used = heap_bytes_used;
        self.allocation_count = allocation_count;
        Ok(())
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

    fn traceback_snapshots(&self) -> Vec<TraceFrameSnapshot> {
        self.traceback_frames()
            .into_iter()
            .map(|frame| TraceFrameSnapshot {
                function_name: frame.function_name,
                span: frame.span,
            })
            .collect()
    }

    fn compose_traceback(&self, origin: &[TraceFrameSnapshot]) -> Vec<TraceFrame> {
        let mut frames = self.traceback_frames();
        for frame in origin {
            let candidate = TraceFrame {
                function_name: frame.function_name.clone(),
                span: frame.span,
            };
            if !frames.iter().any(|existing| {
                existing.function_name == candidate.function_name && existing.span == candidate.span
            }) {
                frames.push(candidate);
            }
        }
        frames
    }

    fn current_async_boundary_index(&self) -> Option<usize> {
        self.frames
            .iter()
            .rposition(|frame| frame.async_promise.is_some())
    }

    fn promise_outcome(&self, promise: PromiseKey) -> JsliteResult<Option<PromiseOutcome>> {
        let promise = self
            .promises
            .get(promise)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?;
        Ok(match &promise.state {
            PromiseState::Pending => None,
            PromiseState::Fulfilled(value) => Some(PromiseOutcome::Fulfilled(value.clone())),
            PromiseState::Rejected(rejection) => Some(PromiseOutcome::Rejected(rejection.clone())),
        })
    }

    fn refresh_promise_accounting(&mut self, key: PromiseKey) -> JsliteResult<()> {
        let (old_bytes, new_bytes) = {
            let promise = self
                .promises
                .get(key)
                .ok_or_else(|| JsliteError::runtime("promise missing"))?;
            (promise.accounted_bytes, measure_promise_bytes(promise))
        };
        self.apply_heap_delta(old_bytes, new_bytes)?;
        self.promises
            .get_mut(key)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .accounted_bytes = new_bytes;
        Ok(())
    }

    fn coerce_to_promise(&mut self, value: Value) -> JsliteResult<PromiseKey> {
        match value {
            Value::Promise(promise) => Ok(promise),
            other => self.insert_promise(PromiseState::Fulfilled(other)),
        }
    }

    fn attach_awaiter(
        &mut self,
        promise: PromiseKey,
        continuation: AsyncContinuation,
    ) -> JsliteResult<()> {
        self.promises
            .get_mut(promise)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .awaiters
            .push(continuation);
        self.refresh_promise_accounting(promise)
    }

    fn attach_dependent(&mut self, promise: PromiseKey, dependent: PromiseKey) -> JsliteResult<()> {
        self.promises
            .get_mut(promise)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .dependents
            .push(dependent);
        self.refresh_promise_accounting(promise)
    }

    fn attach_promise_reaction(
        &mut self,
        promise: PromiseKey,
        reaction: PromiseReaction,
    ) -> JsliteResult<()> {
        match self.promise_outcome(promise)? {
            Some(outcome) => self.schedule_promise_reaction(reaction, outcome),
            None => {
                self.promises
                    .get_mut(promise)
                    .ok_or_else(|| JsliteError::runtime("promise missing"))?
                    .reactions
                    .push(reaction);
                self.refresh_promise_accounting(promise)
            }
        }
    }

    fn schedule_promise_reaction(
        &mut self,
        reaction: PromiseReaction,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        self.microtasks
            .push_back(MicrotaskJob::PromiseReaction { reaction, outcome });
        Ok(())
    }

    fn settle_promise_with_outcome(
        &mut self,
        promise: PromiseKey,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        let (awaiters, dependents, reactions) = {
            let promise_ref = self
                .promises
                .get_mut(promise)
                .ok_or_else(|| JsliteError::runtime("promise missing"))?;
            if !matches!(promise_ref.state, PromiseState::Pending) {
                return Ok(());
            }
            promise_ref.state = match &outcome {
                PromiseOutcome::Fulfilled(value) => PromiseState::Fulfilled(value.clone()),
                PromiseOutcome::Rejected(rejection) => PromiseState::Rejected(rejection.clone()),
            };
            promise_ref.driver = None;
            (
                std::mem::take(&mut promise_ref.awaiters),
                std::mem::take(&mut promise_ref.dependents),
                std::mem::take(&mut promise_ref.reactions),
            )
        };
        self.refresh_promise_accounting(promise)?;
        for continuation in awaiters {
            self.microtasks.push_back(MicrotaskJob::ResumeAsync {
                continuation,
                outcome: outcome.clone(),
            });
        }
        for dependent in dependents {
            self.resolve_promise_with_outcome(dependent, outcome.clone())?;
        }
        for reaction in reactions {
            self.schedule_promise_reaction(reaction, outcome.clone())?;
        }
        Ok(())
    }

    fn resolve_promise_with_outcome(
        &mut self,
        promise: PromiseKey,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        match outcome {
            PromiseOutcome::Fulfilled(value) => self.resolve_promise(promise, value),
            PromiseOutcome::Rejected(rejection) => self.reject_promise(promise, rejection),
        }
    }

    fn resolve_promise(&mut self, promise: PromiseKey, value: Value) -> JsliteResult<()> {
        if let Value::Promise(source) = value {
            if source == promise {
                let error_value =
                    self.value_from_runtime_message("TypeError: promise cannot resolve to itself")?;
                return self.reject_promise(
                    promise,
                    PromiseRejection {
                        value: error_value,
                        span: None,
                        traceback: self.traceback_snapshots(),
                    },
                );
            }
            match self.promise_outcome(source)? {
                Some(outcome) => self.resolve_promise_with_outcome(promise, outcome),
                None => self.attach_dependent(source, promise),
            }
        } else {
            self.settle_promise_with_outcome(promise, PromiseOutcome::Fulfilled(value))
        }
    }

    fn reject_promise(
        &mut self,
        promise: PromiseKey,
        rejection: PromiseRejection,
    ) -> JsliteResult<()> {
        self.settle_promise_with_outcome(promise, PromiseOutcome::Rejected(rejection))
    }

    fn suspend_async_await(&mut self, value: Value) -> JsliteResult<()> {
        let boundary = self.current_async_boundary_index().ok_or_else(|| {
            JsliteError::runtime("await is only supported inside async functions")
        })?;
        let promise = self.coerce_to_promise(value)?;
        let continuation = AsyncContinuation {
            frames: self.frames.split_off(boundary),
        };
        match self.promise_outcome(promise)? {
            Some(outcome) => self.microtasks.push_back(MicrotaskJob::ResumeAsync {
                continuation,
                outcome,
            }),
            None => self.attach_awaiter(promise, continuation)?,
        }
        Ok(())
    }

    fn promise_reaction_target(&self, reaction: &PromiseReaction) -> PromiseKey {
        match reaction {
            PromiseReaction::Then { target, .. }
            | PromiseReaction::Finally { target, .. }
            | PromiseReaction::FinallyPassThrough { target, .. }
            | PromiseReaction::Combinator { target, .. } => *target,
        }
    }

    fn runtime_error_to_promise_rejection(
        &mut self,
        error: JsliteError,
    ) -> JsliteResult<PromiseRejection> {
        match error {
            JsliteError::Message {
                kind: DiagnosticKind::Runtime,
                message,
                span,
                traceback,
            } => Ok(PromiseRejection {
                value: self.value_from_runtime_message(&message)?,
                span,
                traceback: if traceback.is_empty() {
                    self.traceback_snapshots()
                } else {
                    traceback
                        .into_iter()
                        .map(|frame| TraceFrameSnapshot {
                            function_name: frame.function_name,
                            span: frame.span,
                        })
                        .collect()
                },
            }),
            other => Err(other),
        }
    }

    fn reject_promise_from_error(
        &mut self,
        target: PromiseKey,
        error: JsliteError,
    ) -> JsliteResult<()> {
        let rejection = self.runtime_error_to_promise_rejection(error)?;
        self.reject_promise(target, rejection)
    }

    fn invoke_promise_handler(
        &mut self,
        handler: Value,
        args: &[Value],
        target: PromiseKey,
    ) -> JsliteResult<()> {
        match handler {
            Value::Closure(closure) => {
                let closure = self
                    .closures
                    .get(closure)
                    .cloned()
                    .ok_or_else(|| JsliteError::runtime("closure not found"))?;
                let env = self.new_env(Some(closure.env))?;
                let (is_async, function_id) = self
                    .program
                    .functions
                    .get(closure.function_id)
                    .map(|function| (function.is_async, closure.function_id))
                    .ok_or_else(|| JsliteError::runtime("function not found"))?;
                if is_async {
                    let bridge = self.insert_promise(PromiseState::Pending)?;
                    self.attach_dependent(bridge, target)?;
                    self.push_frame(function_id, env, args, Value::Undefined, Some(bridge))?;
                } else {
                    self.push_frame(function_id, env, args, Value::Undefined, Some(target))?;
                }
                Ok(())
            }
            Value::BuiltinFunction(function) => {
                let value = self.call_builtin(function, Value::Undefined, args)?;
                self.resolve_promise(target, value)
            }
            Value::HostFunction(capability) => {
                let outstanding =
                    self.pending_host_calls.len() + usize::from(self.suspended_host_call.is_some());
                if outstanding >= self.limits.max_outstanding_host_calls {
                    return Err(limit_error("outstanding host-call limit exhausted"));
                }
                let args = args
                    .iter()
                    .cloned()
                    .map(|value| self.value_to_structured(value))
                    .collect::<JsliteResult<Vec<_>>>()?;
                let promise = self.insert_promise(PromiseState::Pending)?;
                self.attach_dependent(promise, target)?;
                self.pending_host_calls.push_back(PendingHostCall {
                    capability,
                    args,
                    promise,
                    resume_behavior: ResumeBehavior::Value,
                    traceback: self.traceback_snapshots(),
                });
                Ok(())
            }
            _ => Err(JsliteError::runtime("value is not callable")),
        }
    }

    fn make_promise_all_settled_result(
        &mut self,
        result: PromiseSettledResult,
    ) -> JsliteResult<Value> {
        let properties = match result {
            PromiseSettledResult::Fulfilled(value) => IndexMap::from([
                ("status".to_string(), Value::String("fulfilled".to_string())),
                ("value".to_string(), value),
            ]),
            PromiseSettledResult::Rejected(reason) => IndexMap::from([
                ("status".to_string(), Value::String("rejected".to_string())),
                ("reason".to_string(), reason),
            ]),
        };
        Ok(Value::Object(
            self.insert_object(properties, ObjectKind::Plain)?,
        ))
    }

    fn make_aggregate_error(&mut self, reasons: Vec<Value>) -> JsliteResult<Value> {
        let error = self.make_error_object(
            "AggregateError",
            &[Value::String("All promises were rejected".to_string())],
            None,
            None,
        )?;
        let errors = Value::Array(self.insert_array(reasons, IndexMap::new())?);
        self.set_property(error.clone(), Value::String("errors".to_string()), errors)?;
        Ok(error)
    }

    fn activate_promise_combinator(
        &mut self,
        target: PromiseKey,
        index: usize,
        kind: PromiseCombinatorKind,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        if self.promise_outcome(target)?.is_some() {
            return Ok(());
        }
        match kind {
            PromiseCombinatorKind::Race => self.resolve_promise_with_outcome(target, outcome),
            PromiseCombinatorKind::All => {
                let mut resolved_values = None;
                let mut rejection = None;
                {
                    let promise = self
                        .promises
                        .get_mut(target)
                        .ok_or_else(|| JsliteError::runtime("promise missing"))?;
                    let PromiseState::Pending = promise.state else {
                        return Ok(());
                    };
                    let PromiseDriver::All { remaining, values } = promise
                        .driver
                        .as_mut()
                        .ok_or_else(|| JsliteError::runtime("promise combinator state missing"))?
                    else {
                        return Err(JsliteError::runtime("promise combinator kind mismatch"));
                    };
                    match outcome {
                        PromiseOutcome::Fulfilled(value) => {
                            values[index] = Some(value);
                            *remaining = remaining.saturating_sub(1);
                            if *remaining == 0 {
                                resolved_values = Some(
                                    values
                                        .iter()
                                        .map(|value| value.clone().unwrap_or(Value::Undefined))
                                        .collect::<Vec<_>>(),
                                );
                            }
                        }
                        PromiseOutcome::Rejected(reason) => rejection = Some(reason),
                    }
                }
                self.refresh_promise_accounting(target)?;
                if let Some(rejection) = rejection {
                    self.reject_promise(target, rejection)?;
                } else if let Some(values) = resolved_values {
                    let array = Value::Array(self.insert_array(values, IndexMap::new())?);
                    self.resolve_promise(target, array)?;
                }
                Ok(())
            }
            PromiseCombinatorKind::AllSettled => {
                let mut settled_results = None;
                {
                    let promise = self
                        .promises
                        .get_mut(target)
                        .ok_or_else(|| JsliteError::runtime("promise missing"))?;
                    let PromiseState::Pending = promise.state else {
                        return Ok(());
                    };
                    let PromiseDriver::AllSettled { remaining, results } = promise
                        .driver
                        .as_mut()
                        .ok_or_else(|| JsliteError::runtime("promise combinator state missing"))?
                    else {
                        return Err(JsliteError::runtime("promise combinator kind mismatch"));
                    };
                    results[index] = Some(match outcome {
                        PromiseOutcome::Fulfilled(value) => PromiseSettledResult::Fulfilled(value),
                        PromiseOutcome::Rejected(reason) => {
                            PromiseSettledResult::Rejected(reason.value)
                        }
                    });
                    *remaining = remaining.saturating_sub(1);
                    if *remaining == 0 {
                        settled_results = Some(
                            results
                                .iter()
                                .map(|result| {
                                    result.clone().unwrap_or(PromiseSettledResult::Fulfilled(
                                        Value::Undefined,
                                    ))
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                }
                self.refresh_promise_accounting(target)?;
                if let Some(results) = settled_results {
                    let mut values = Vec::with_capacity(results.len());
                    for result in results {
                        values.push(self.make_promise_all_settled_result(result)?);
                    }
                    let array = Value::Array(self.insert_array(values, IndexMap::new())?);
                    self.resolve_promise(target, array)?;
                }
                Ok(())
            }
            PromiseCombinatorKind::Any => {
                let mut rejection_values = None;
                let mut fulfillment = None;
                {
                    let promise = self
                        .promises
                        .get_mut(target)
                        .ok_or_else(|| JsliteError::runtime("promise missing"))?;
                    let PromiseState::Pending = promise.state else {
                        return Ok(());
                    };
                    let PromiseDriver::Any { remaining, reasons } = promise
                        .driver
                        .as_mut()
                        .ok_or_else(|| JsliteError::runtime("promise combinator state missing"))?
                    else {
                        return Err(JsliteError::runtime("promise combinator kind mismatch"));
                    };
                    match outcome {
                        PromiseOutcome::Fulfilled(value) => fulfillment = Some(value),
                        PromiseOutcome::Rejected(reason) => {
                            reasons[index] = Some(reason.value);
                            *remaining = remaining.saturating_sub(1);
                            if *remaining == 0 {
                                rejection_values = Some(
                                    reasons
                                        .iter()
                                        .map(|value| value.clone().unwrap_or(Value::Undefined))
                                        .collect::<Vec<_>>(),
                                );
                            }
                        }
                    }
                }
                self.refresh_promise_accounting(target)?;
                if let Some(value) = fulfillment {
                    self.resolve_promise(target, value)?;
                } else if let Some(reasons) = rejection_values {
                    let rejection = PromiseRejection {
                        value: self.make_aggregate_error(reasons)?,
                        span: None,
                        traceback: self.traceback_snapshots(),
                    };
                    self.reject_promise(target, rejection)?;
                }
                Ok(())
            }
        }
    }

    fn activate_promise_reaction(
        &mut self,
        reaction: PromiseReaction,
        outcome: PromiseOutcome,
    ) -> JsliteResult<()> {
        let target = self.promise_reaction_target(&reaction);
        let result = (|| match reaction {
            PromiseReaction::Then {
                target,
                on_fulfilled,
                on_rejected,
            } => match outcome {
                PromiseOutcome::Fulfilled(value) => {
                    if let Some(handler) = on_fulfilled {
                        self.invoke_promise_handler(handler, &[value], target)
                    } else {
                        self.resolve_promise(target, value)
                    }
                }
                PromiseOutcome::Rejected(rejection) => {
                    if let Some(handler) = on_rejected {
                        self.invoke_promise_handler(handler, &[rejection.value], target)
                    } else {
                        self.reject_promise(target, rejection)
                    }
                }
            },
            PromiseReaction::Finally { target, callback } => {
                if let Some(callback) = callback {
                    let bridge = self.insert_promise(PromiseState::Pending)?;
                    self.attach_promise_reaction(
                        bridge,
                        PromiseReaction::FinallyPassThrough {
                            target,
                            original_outcome: outcome,
                        },
                    )?;
                    self.invoke_promise_handler(callback, &[], bridge)
                } else {
                    self.resolve_promise_with_outcome(target, outcome)
                }
            }
            PromiseReaction::FinallyPassThrough {
                target,
                original_outcome,
            } => match outcome {
                PromiseOutcome::Fulfilled(_) => {
                    self.resolve_promise_with_outcome(target, original_outcome)
                }
                PromiseOutcome::Rejected(rejection) => self.reject_promise(target, rejection),
            },
            PromiseReaction::Combinator {
                target,
                index,
                kind,
            } => self.activate_promise_combinator(target, index, kind, outcome),
        })();

        match result {
            Ok(()) => Ok(()),
            Err(error) => self.reject_promise_from_error(target, error),
        }
    }

    fn activate_microtask(&mut self, job: MicrotaskJob) -> JsliteResult<()> {
        if !self.frames.is_empty() {
            return Err(JsliteError::runtime(
                "microtask checkpoint ran while frames were still active",
            ));
        }
        match job {
            MicrotaskJob::ResumeAsync {
                continuation,
                outcome,
            } => {
                self.frames = continuation.frames;
                match outcome {
                    PromiseOutcome::Fulfilled(value) => {
                        let frame = self.frames.last_mut().ok_or_else(|| {
                            JsliteError::runtime("async continuation resumed without frames")
                        })?;
                        frame.stack.push(value);
                    }
                    PromiseOutcome::Rejected(rejection) => {
                        match self.raise_exception_with_origin(
                            rejection.value,
                            rejection.span,
                            Some(rejection.traceback),
                        )? {
                            StepAction::Continue => {}
                            StepAction::Return(_) => {}
                        }
                    }
                }
            }
            MicrotaskJob::PromiseReaction { reaction, outcome } => {
                self.activate_promise_reaction(reaction, outcome)?;
            }
        }
        Ok(())
    }

    fn has_pending_async_work(&self) -> bool {
        self.suspended_host_call.is_some()
            || !self.pending_host_calls.is_empty()
            || !self.microtasks.is_empty()
            || self.promises.values().any(|promise| {
                matches!(promise.state, PromiseState::Pending)
                    && (!promise.awaiters.is_empty()
                        || !promise.dependents.is_empty()
                        || !promise.reactions.is_empty())
            })
    }

    fn root_error_from_rejection(&self, rejection: PromiseRejection) -> JsliteResult<JsliteError> {
        Ok(JsliteError::Message {
            kind: DiagnosticKind::Runtime,
            message: self.render_exception(&rejection.value)?,
            span: rejection.span,
            traceback: self.compose_traceback(&rejection.traceback),
        })
    }

    fn suspend_host_request(&mut self, request: PendingHostCall) -> ExecutionStep {
        let capability = request.capability.clone();
        let args = request.args.clone();
        self.suspended_host_call = Some(request);
        ExecutionStep::Suspended(Box::new(Suspension {
            capability,
            args,
            snapshot: ExecutionSnapshot {
                runtime: self.clone(),
            },
        }))
    }

    fn process_idle_state(&mut self) -> JsliteResult<Option<ExecutionStep>> {
        if let Some(job) = self.microtasks.pop_front() {
            self.activate_microtask(job)?;
            return Ok(None);
        }
        if let Some(request) = self.pending_host_calls.pop_front() {
            return Ok(Some(self.suspend_host_request(request)));
        }
        if let Some(root_result) = self.root_result.clone() {
            return match root_result {
                Value::Promise(promise) => match self.promise_outcome(promise)? {
                    Some(PromiseOutcome::Fulfilled(value)) => Ok(Some(ExecutionStep::Completed(
                        self.value_to_structured(value)?,
                    ))),
                    Some(PromiseOutcome::Rejected(rejection)) => {
                        Err(self.root_error_from_rejection(rejection)?)
                    }
                    None => Err(JsliteError::runtime(
                        "async root promise could not make progress",
                    )),
                },
                value => {
                    if self.has_pending_async_work() {
                        return Err(JsliteError::runtime(
                            "async execution became idle with pending work",
                        ));
                    }
                    Ok(Some(ExecutionStep::Completed(
                        self.value_to_structured(value)?,
                    )))
                }
            };
        }
        if self.has_pending_async_work() {
            return Err(JsliteError::runtime(
                "async execution became idle before producing a root result",
            ));
        }
        Err(JsliteError::runtime("vm lost all frames"))
    }

    fn resume(&mut self, payload: ResumePayload) -> JsliteResult<ExecutionStep> {
        if let Err(error) = self.check_cancellation() {
            if let Some(request) = self.suspended_host_call.as_ref() {
                return Err(error.with_traceback(self.compose_traceback(&request.traceback)));
            }
            return Err(self.annotate_runtime_error(error));
        }
        self.collect_garbage()
            .map_err(|error| self.annotate_runtime_error(error))?;
        if let Some(request) = self.suspended_host_call.take() {
            let outcome = match payload {
                ResumePayload::Value(value) => {
                    let value = match request.resume_behavior {
                        ResumeBehavior::Value => self
                            .value_from_structured(value)
                            .map_err(|error| self.annotate_runtime_error(error))?,
                        ResumeBehavior::Undefined => Value::Undefined,
                    };
                    PromiseOutcome::Fulfilled(value)
                }
                ResumePayload::Error(error) => PromiseOutcome::Rejected(PromiseRejection {
                    value: self
                        .value_from_host_error(error)
                        .map_err(|error| self.annotate_runtime_error(error))?,
                    span: None,
                    traceback: Vec::new(),
                }),
                ResumePayload::Cancelled => {
                    return Err(limit_error("execution cancelled")
                        .with_traceback(self.compose_traceback(&request.traceback)));
                }
            };
            self.resolve_promise_with_outcome(request.promise, outcome)
                .map_err(|error| self.annotate_runtime_error(error))?;
            return self.run();
        }
        match payload {
            ResumePayload::Value(value) => {
                let value = match self.pending_resume_behavior {
                    ResumeBehavior::Value => self
                        .value_from_structured(value)
                        .map_err(|error| self.annotate_runtime_error(error))?,
                    ResumeBehavior::Undefined => Value::Undefined,
                };
                self.pending_resume_behavior = ResumeBehavior::Value;
                let Some(frame) = self.frames.last_mut() else {
                    return Err(self.annotate_runtime_error(JsliteError::runtime(
                        "no suspended frame available",
                    )));
                };
                frame.stack.push(value);
            }
            ResumePayload::Error(error) => {
                self.pending_resume_behavior = ResumeBehavior::Value;
                let value = self
                    .value_from_host_error(error)
                    .map_err(|error| self.annotate_runtime_error(error))?;
                match self.raise_exception(value, None) {
                    Ok(StepAction::Continue) => return self.run(),
                    Ok(StepAction::Return(step)) => return Ok(step),
                    Err(error) => return Err(self.annotate_runtime_error(error)),
                }
            }
            ResumePayload::Cancelled => {
                return Err(self.annotate_runtime_error(limit_error("execution cancelled")));
            }
        }
        self.run()
    }

    fn run(&mut self) -> JsliteResult<ExecutionStep> {
        loop {
            self.check_cancellation()
                .map_err(|error| self.annotate_runtime_error(error))?;
            if self.frames.is_empty() {
                match self.process_idle_state() {
                    Ok(Some(step)) => return Ok(step),
                    Ok(None) => continue,
                    Err(error) => return Err(self.annotate_runtime_error(error)),
                }
            }
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
            self.collect_garbage_before_instruction(&instruction)
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
                        let env = self.new_env(Some(current_env))?;
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
                        let closure = self.insert_closure(function_id, env)?;
                        self.frames[frame_index].stack.push(Value::Closure(closure));
                    }
                    Instruction::MakeArray { count } => {
                        let values = pop_many(&mut self.frames[frame_index].stack, count)?;
                        let array = self.insert_array(values, IndexMap::new())?;
                        self.frames[frame_index].stack.push(Value::Array(array));
                    }
                    Instruction::MakeObject { keys } => {
                        let values = pop_many(&mut self.frames[frame_index].stack, keys.len())?;
                        let mut properties = IndexMap::new();
                        for (key, value) in keys.into_iter().zip(values.into_iter()) {
                            properties.insert(property_name_to_key(&key), value);
                        }
                        let object = self.insert_object(properties, ObjectKind::Plain)?;
                        self.frames[frame_index].stack.push(Value::Object(object));
                    }
                    Instruction::CreateIterator => {
                        let iterable = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let iterator = self.create_iterator(iterable)?;
                        self.frames[frame_index].stack.push(iterator);
                    }
                    Instruction::IteratorNext => {
                        let iterator = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        let (value, done) = self.iterator_next(iterator)?;
                        self.frames[frame_index].stack.push(value);
                        self.frames[frame_index].stack.push(Value::Bool(done));
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
                    Instruction::PushHandler { catch, finally } => {
                        let frame = &mut self.frames[frame_index];
                        frame.handlers.push(ExceptionHandler {
                            catch,
                            finally,
                            env: frame.env,
                            scope_stack_len: frame.scope_stack.len(),
                            stack_len: frame.stack.len(),
                        });
                    }
                    Instruction::PopHandler => {
                        self.frames[frame_index]
                            .handlers
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("handler stack underflow"))?;
                    }
                    Instruction::EnterFinally { exit } => {
                        let completion_index = self.frames[frame_index]
                            .pending_completions
                            .len()
                            .checked_sub(1)
                            .ok_or_else(|| JsliteError::runtime("missing pending completion"))?;
                        self.frames[frame_index]
                            .active_finally
                            .push(ActiveFinallyState {
                                completion_index,
                                exit,
                            });
                    }
                    Instruction::BeginCatch => {
                        let value = self.frames[frame_index]
                            .pending_exception
                            .take()
                            .ok_or_else(|| JsliteError::runtime("missing pending exception"))?;
                        self.frames[frame_index].stack.push(value);
                    }
                    Instruction::Throw { span } => {
                        let value = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        return self.raise_exception(value, Some(span));
                    }
                    Instruction::PushPendingJump {
                        target,
                        target_handler_depth,
                        target_scope_depth,
                    } => {
                        self.store_completion(
                            frame_index,
                            CompletionRecord::Jump {
                                target,
                                target_handler_depth,
                                target_scope_depth,
                            },
                        )?;
                    }
                    Instruction::PushPendingReturn => {
                        let value = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        self.store_completion(frame_index, CompletionRecord::Return(value))?;
                    }
                    Instruction::PushPendingThrow => {
                        let value = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        self.store_completion(frame_index, CompletionRecord::Throw(value))?;
                    }
                    Instruction::ContinuePending => {
                        let marker = self.frames[frame_index]
                            .active_finally
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("missing active finally state"))?;
                        if marker.completion_index
                            >= self.frames[frame_index].pending_completions.len()
                        {
                            return Err(JsliteError::runtime(
                                "active finally references missing completion",
                            ));
                        }
                        let completion = self.frames[frame_index]
                            .pending_completions
                            .remove(marker.completion_index);
                        return self.resume_completion(completion);
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
                            RunState::StartedAsync(value) => {
                                self.frames[frame_index].stack.push(value);
                            }
                            RunState::Suspended {
                                capability,
                                args,
                                resume_behavior,
                            } => {
                                self.pending_resume_behavior = resume_behavior;
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
                    Instruction::Await => {
                        let value = self.frames[frame_index]
                            .stack
                            .pop()
                            .ok_or_else(|| JsliteError::runtime("stack underflow"))?;
                        self.suspend_async_await(value)?;
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
                        return self.complete_return(value);
                    }
                }
                Ok(StepAction::Continue)
            })();

            let action = match action {
                Ok(action) => action,
                Err(error) => match self.handle_runtime_fault(error) {
                    Ok(action) => action,
                    Err(error) => return Err(self.annotate_runtime_error(error)),
                },
            };

            match action {
                StepAction::Continue => {}
                StepAction::Return(step) => return Ok(step),
            }
        }
    }

    fn handle_runtime_fault(&mut self, error: JsliteError) -> JsliteResult<StepAction> {
        match error {
            JsliteError::Message {
                kind: DiagnosticKind::Runtime,
                message,
                span,
                ..
            } => {
                let value = self.value_from_runtime_message(&message)?;
                self.raise_exception(value, span)
            }
            other => Err(other),
        }
    }

    fn store_completion(
        &mut self,
        frame_index: usize,
        completion: CompletionRecord,
    ) -> JsliteResult<()> {
        let completion_index = self.frames[frame_index]
            .active_finally
            .last()
            .map(|active| active.completion_index);
        if let Some(completion_index) = completion_index {
            if completion_index >= self.frames[frame_index].pending_completions.len() {
                return Err(JsliteError::runtime(
                    "active finally references missing completion",
                ));
            }
            self.frames[frame_index].pending_completions[completion_index] = completion;
        } else {
            self.frames[frame_index]
                .pending_completions
                .push(completion);
        }
        Ok(())
    }

    fn restore_handler_state(
        &mut self,
        frame_index: usize,
        handler: &ExceptionHandler,
    ) -> JsliteResult<()> {
        let frame = &mut self.frames[frame_index];
        frame.env = handler.env;
        frame.scope_stack.truncate(handler.scope_stack_len);
        frame.stack.truncate(handler.stack_len);
        Ok(())
    }

    fn raise_exception(
        &mut self,
        value: Value,
        span: Option<SourceSpan>,
    ) -> JsliteResult<StepAction> {
        self.raise_exception_with_origin(value, span, None)
    }

    fn raise_exception_with_origin(
        &mut self,
        value: Value,
        span: Option<SourceSpan>,
        origin_traceback: Option<Vec<TraceFrameSnapshot>>,
    ) -> JsliteResult<StepAction> {
        let traceback = match origin_traceback.as_ref() {
            Some(origin) => self.compose_traceback(origin),
            None => self.traceback_frames(),
        };
        let thrown = value;

        loop {
            let Some(frame_index) = self.frames.len().checked_sub(1) else {
                return Err(JsliteError::runtime("vm lost all frames"));
            };

            if let Some(handler_index) = self.frames[frame_index]
                .handlers
                .iter()
                .rposition(|handler| handler.catch.is_some() || handler.finally.is_some())
            {
                let handler = self.frames[frame_index].handlers[handler_index].clone();
                self.frames[frame_index].handlers.truncate(handler_index);
                self.restore_handler_state(frame_index, &handler)?;

                if let Some(catch_ip) = handler.catch {
                    self.frames[frame_index].pending_exception = Some(thrown);
                    self.frames[frame_index].ip = catch_ip;
                    return Ok(StepAction::Continue);
                }

                if let Some(finally_ip) = handler.finally {
                    self.frames[frame_index]
                        .pending_completions
                        .push(CompletionRecord::Throw(thrown));
                    self.frames[frame_index].ip = finally_ip;
                    return Ok(StepAction::Continue);
                }
            }

            if let Some(active) = self.frames[frame_index].active_finally.last().cloned() {
                if active.completion_index >= self.frames[frame_index].pending_completions.len() {
                    return Err(JsliteError::runtime(
                        "active finally references missing completion",
                    ));
                }
                self.frames[frame_index].pending_completions[active.completion_index] =
                    CompletionRecord::Throw(thrown);
                self.frames[frame_index].ip = active.exit;
                return Ok(StepAction::Continue);
            }

            if let Some(async_promise) = self.frames[frame_index].async_promise {
                self.frames.pop();
                self.reject_promise(
                    async_promise,
                    PromiseRejection {
                        value: thrown,
                        span,
                        traceback: traceback
                            .iter()
                            .map(|frame| TraceFrameSnapshot {
                                function_name: frame.function_name.clone(),
                                span: frame.span,
                            })
                            .collect(),
                    },
                )?;
                return Ok(StepAction::Continue);
            }

            if self.frames.len() == 1 {
                let message = self.render_exception(&thrown)?;
                return Err(JsliteError::Message {
                    kind: DiagnosticKind::Runtime,
                    message,
                    span,
                    traceback,
                });
            }

            self.frames.pop();
        }
    }

    fn resume_completion(&mut self, completion: CompletionRecord) -> JsliteResult<StepAction> {
        match completion {
            CompletionRecord::Throw(value) => self.raise_exception(value, None),
            CompletionRecord::Jump {
                target,
                target_handler_depth,
                target_scope_depth,
            } => self.resume_nonthrow_completion(
                target_handler_depth,
                target_scope_depth,
                CompletionRecord::Jump {
                    target,
                    target_handler_depth,
                    target_scope_depth,
                },
            ),
            CompletionRecord::Return(value) => {
                self.resume_nonthrow_completion(0, 0, CompletionRecord::Return(value))
            }
        }
    }

    fn resume_nonthrow_completion(
        &mut self,
        target_handler_depth: usize,
        target_scope_depth: usize,
        completion: CompletionRecord,
    ) -> JsliteResult<StepAction> {
        let frame_index = self
            .frames
            .len()
            .checked_sub(1)
            .ok_or_else(|| JsliteError::runtime("vm lost all frames"))?;
        let current_depth = self.frames[frame_index].handlers.len();
        if target_handler_depth > current_depth {
            return Err(JsliteError::runtime(
                "completion targets missing handler depth",
            ));
        }
        if target_scope_depth > self.frames[frame_index].scope_stack.len() {
            return Err(JsliteError::runtime(
                "completion targets missing scope depth",
            ));
        }

        let restore_state = if target_handler_depth < current_depth {
            self.frames[frame_index]
                .handlers
                .get(target_handler_depth)
                .cloned()
        } else {
            None
        };

        if let Some(handler_index) = (target_handler_depth..current_depth)
            .rev()
            .find(|index| self.frames[frame_index].handlers[*index].finally.is_some())
        {
            let handler = self.frames[frame_index].handlers[handler_index].clone();
            self.frames[frame_index].handlers.truncate(handler_index);
            self.restore_handler_state(frame_index, &handler)?;
            self.frames[frame_index]
                .pending_completions
                .push(completion);
            self.frames[frame_index].ip = handler
                .finally
                .ok_or_else(|| JsliteError::runtime("missing finally target"))?;
            return Ok(StepAction::Continue);
        }

        if let Some(handler) = restore_state.as_ref() {
            self.restore_handler_state(frame_index, handler)?;
        }
        self.frames[frame_index]
            .handlers
            .truncate(target_handler_depth);

        match completion {
            CompletionRecord::Jump { target, .. } => {
                if self.frames[frame_index].scope_stack.len() < target_scope_depth {
                    return Err(JsliteError::runtime(
                        "completion targets missing scope depth",
                    ));
                }
                while self.frames[frame_index].scope_stack.len() > target_scope_depth {
                    let restored = self.frames[frame_index]
                        .scope_stack
                        .pop()
                        .ok_or_else(|| JsliteError::runtime("scope stack underflow"))?;
                    self.frames[frame_index].env = restored;
                }
                self.frames[frame_index].ip = target;
                Ok(StepAction::Continue)
            }
            CompletionRecord::Return(value) => self.complete_return(value),
            CompletionRecord::Throw(_) => unreachable!(),
        }
    }

    fn complete_return(&mut self, value: Value) -> JsliteResult<StepAction> {
        let frame = self
            .frames
            .pop()
            .ok_or_else(|| JsliteError::runtime("vm lost all frames"))?;
        if let Some(async_promise) = frame.async_promise {
            self.resolve_promise(async_promise, value)?;
            return Ok(StepAction::Continue);
        }
        if let Some(parent) = self.frames.last_mut() {
            parent.stack.push(value);
            Ok(StepAction::Continue)
        } else {
            self.root_result = Some(value);
            Ok(StepAction::Continue)
        }
    }

    fn collect_garbage_before_instruction(
        &mut self,
        instruction: &Instruction,
    ) -> JsliteResult<()> {
        if instruction_may_allocate(instruction) {
            self.collect_garbage()?;
        }
        Ok(())
    }

    fn collect_garbage(&mut self) -> JsliteResult<GarbageCollectionStats> {
        let baseline_bytes = self.heap_bytes_used;
        let baseline_allocations = self.allocation_count;
        let marks = self.mark_reachable_heap()?;

        self.sweep_unreachable_envs(&marks);
        self.sweep_unreachable_cells(&marks);
        self.sweep_unreachable_objects(&marks);
        self.sweep_unreachable_arrays(&marks);
        self.sweep_unreachable_maps(&marks);
        self.sweep_unreachable_sets(&marks);
        self.sweep_unreachable_iterators(&marks);
        self.sweep_unreachable_closures(&marks);
        self.sweep_unreachable_promises(&marks);

        let (heap_bytes_used, allocation_count) = self
            .recompute_accounting_totals()
            .map_err(JsliteError::runtime)?;
        self.heap_bytes_used = heap_bytes_used;
        self.allocation_count = allocation_count;

        Ok(GarbageCollectionStats {
            reclaimed_bytes: baseline_bytes.saturating_sub(heap_bytes_used),
            reclaimed_allocations: baseline_allocations.saturating_sub(allocation_count),
        })
    }

    fn mark_reachable_heap(&self) -> JsliteResult<GarbageCollectionMarks> {
        let mut marks = GarbageCollectionMarks::default();
        let mut worklist = GarbageCollectionWorklist::default();

        self.mark_env(self.globals, &mut marks, &mut worklist);
        if let Some(root_result) = &self.root_result {
            self.mark_value(root_result, &mut marks, &mut worklist);
        }
        for frame in &self.frames {
            self.mark_frame_roots(frame, &mut marks, &mut worklist);
        }
        for job in &self.microtasks {
            match job {
                MicrotaskJob::ResumeAsync {
                    continuation,
                    outcome,
                } => {
                    for frame in &continuation.frames {
                        self.mark_frame_roots(frame, &mut marks, &mut worklist);
                    }
                    match outcome {
                        PromiseOutcome::Fulfilled(value) => {
                            self.mark_value(value, &mut marks, &mut worklist);
                        }
                        PromiseOutcome::Rejected(rejection) => {
                            self.mark_value(&rejection.value, &mut marks, &mut worklist);
                        }
                    }
                }
                MicrotaskJob::PromiseReaction { reaction, outcome } => {
                    self.mark_promise(
                        self.promise_reaction_target(reaction),
                        &mut marks,
                        &mut worklist,
                    );
                    match reaction {
                        PromiseReaction::Then {
                            on_fulfilled,
                            on_rejected,
                            ..
                        } => {
                            if let Some(handler) = on_fulfilled {
                                self.mark_value(handler, &mut marks, &mut worklist);
                            }
                            if let Some(handler) = on_rejected {
                                self.mark_value(handler, &mut marks, &mut worklist);
                            }
                        }
                        PromiseReaction::Finally { callback, .. } => {
                            if let Some(callback) = callback {
                                self.mark_value(callback, &mut marks, &mut worklist);
                            }
                        }
                        PromiseReaction::FinallyPassThrough {
                            original_outcome, ..
                        } => match original_outcome {
                            PromiseOutcome::Fulfilled(value) => {
                                self.mark_value(value, &mut marks, &mut worklist);
                            }
                            PromiseOutcome::Rejected(rejection) => {
                                self.mark_value(&rejection.value, &mut marks, &mut worklist);
                            }
                        },
                        PromiseReaction::Combinator { .. } => {}
                    }
                    match outcome {
                        PromiseOutcome::Fulfilled(value) => {
                            self.mark_value(value, &mut marks, &mut worklist);
                        }
                        PromiseOutcome::Rejected(rejection) => {
                            self.mark_value(&rejection.value, &mut marks, &mut worklist);
                        }
                    }
                }
            }
        }
        for request in &self.pending_host_calls {
            self.mark_promise(request.promise, &mut marks, &mut worklist);
        }
        if let Some(request) = &self.suspended_host_call {
            self.mark_promise(request.promise, &mut marks, &mut worklist);
        }

        while !worklist.envs.is_empty()
            || !worklist.cells.is_empty()
            || !worklist.objects.is_empty()
            || !worklist.arrays.is_empty()
            || !worklist.maps.is_empty()
            || !worklist.sets.is_empty()
            || !worklist.iterators.is_empty()
            || !worklist.closures.is_empty()
            || !worklist.promises.is_empty()
        {
            while let Some(key) = worklist.envs.pop() {
                let env = self
                    .envs
                    .get(key)
                    .ok_or_else(|| JsliteError::runtime("gc encountered missing environment"))?;
                if let Some(parent) = env.parent {
                    self.mark_env(parent, &mut marks, &mut worklist);
                }
                for cell in env.bindings.values() {
                    self.mark_cell(*cell, &mut marks, &mut worklist);
                }
            }

            while let Some(key) = worklist.cells.pop() {
                let cell = self
                    .cells
                    .get(key)
                    .ok_or_else(|| JsliteError::runtime("gc encountered missing binding cell"))?;
                self.mark_value(&cell.value, &mut marks, &mut worklist);
            }

            while let Some(key) = worklist.objects.pop() {
                let object = self
                    .objects
                    .get(key)
                    .ok_or_else(|| JsliteError::runtime("gc encountered missing object"))?;
                for value in object.properties.values() {
                    self.mark_value(value, &mut marks, &mut worklist);
                }
            }

            while let Some(key) = worklist.arrays.pop() {
                let array = self
                    .arrays
                    .get(key)
                    .ok_or_else(|| JsliteError::runtime("gc encountered missing array"))?;
                for value in &array.elements {
                    self.mark_value(value, &mut marks, &mut worklist);
                }
                for value in array.properties.values() {
                    self.mark_value(value, &mut marks, &mut worklist);
                }
            }

            while let Some(key) = worklist.maps.pop() {
                let map = self
                    .maps
                    .get(key)
                    .ok_or_else(|| JsliteError::runtime("gc encountered missing map"))?;
                for entry in &map.entries {
                    self.mark_value(&entry.key, &mut marks, &mut worklist);
                    self.mark_value(&entry.value, &mut marks, &mut worklist);
                }
            }

            while let Some(key) = worklist.sets.pop() {
                let set = self
                    .sets
                    .get(key)
                    .ok_or_else(|| JsliteError::runtime("gc encountered missing set"))?;
                for value in &set.entries {
                    self.mark_value(value, &mut marks, &mut worklist);
                }
            }

            while let Some(key) = worklist.iterators.pop() {
                let iterator = self
                    .iterators
                    .get(key)
                    .ok_or_else(|| JsliteError::runtime("gc encountered missing iterator"))?;
                match iterator.state {
                    IteratorState::Array(ref state) => {
                        self.mark_value(&Value::Array(state.array), &mut marks, &mut worklist);
                    }
                    IteratorState::ArrayKeys(ref state)
                    | IteratorState::ArrayEntries(ref state) => {
                        self.mark_value(&Value::Array(state.array), &mut marks, &mut worklist);
                    }
                    IteratorState::String(_) => {}
                    IteratorState::MapEntries(ref state)
                    | IteratorState::MapKeys(ref state)
                    | IteratorState::MapValues(ref state) => {
                        self.mark_value(&Value::Map(state.map), &mut marks, &mut worklist);
                    }
                    IteratorState::SetEntries(ref state) | IteratorState::SetValues(ref state) => {
                        self.mark_value(&Value::Set(state.set), &mut marks, &mut worklist);
                    }
                }
            }

            while let Some(key) = worklist.closures.pop() {
                let closure = self
                    .closures
                    .get(key)
                    .ok_or_else(|| JsliteError::runtime("gc encountered missing closure"))?;
                self.mark_env(closure.env, &mut marks, &mut worklist);
            }

            while let Some(key) = worklist.promises.pop() {
                let promise = self
                    .promises
                    .get(key)
                    .ok_or_else(|| JsliteError::runtime("gc encountered missing promise"))?;
                match &promise.state {
                    PromiseState::Pending => {}
                    PromiseState::Fulfilled(value) => {
                        self.mark_value(value, &mut marks, &mut worklist);
                    }
                    PromiseState::Rejected(rejection) => {
                        self.mark_value(&rejection.value, &mut marks, &mut worklist);
                    }
                }
                for continuation in &promise.awaiters {
                    for frame in &continuation.frames {
                        self.mark_frame_roots(frame, &mut marks, &mut worklist);
                    }
                }
                for dependent in &promise.dependents {
                    self.mark_promise(*dependent, &mut marks, &mut worklist);
                }
                for reaction in &promise.reactions {
                    self.mark_promise(
                        self.promise_reaction_target(reaction),
                        &mut marks,
                        &mut worklist,
                    );
                    match reaction {
                        PromiseReaction::Then {
                            on_fulfilled,
                            on_rejected,
                            ..
                        } => {
                            if let Some(handler) = on_fulfilled {
                                self.mark_value(handler, &mut marks, &mut worklist);
                            }
                            if let Some(handler) = on_rejected {
                                self.mark_value(handler, &mut marks, &mut worklist);
                            }
                        }
                        PromiseReaction::Finally { callback, .. } => {
                            if let Some(callback) = callback {
                                self.mark_value(callback, &mut marks, &mut worklist);
                            }
                        }
                        PromiseReaction::FinallyPassThrough {
                            original_outcome, ..
                        } => match original_outcome {
                            PromiseOutcome::Fulfilled(value) => {
                                self.mark_value(value, &mut marks, &mut worklist);
                            }
                            PromiseOutcome::Rejected(rejection) => {
                                self.mark_value(&rejection.value, &mut marks, &mut worklist);
                            }
                        },
                        PromiseReaction::Combinator { .. } => {}
                    }
                }
                if let Some(driver) = &promise.driver {
                    match driver {
                        PromiseDriver::All { values, .. } => {
                            for value in values.iter().flatten() {
                                self.mark_value(value, &mut marks, &mut worklist);
                            }
                        }
                        PromiseDriver::AllSettled { results, .. } => {
                            for result in results.iter().flatten() {
                                match result {
                                    PromiseSettledResult::Fulfilled(value)
                                    | PromiseSettledResult::Rejected(value) => {
                                        self.mark_value(value, &mut marks, &mut worklist);
                                    }
                                }
                            }
                        }
                        PromiseDriver::Any { reasons, .. } => {
                            for value in reasons.iter().flatten() {
                                self.mark_value(value, &mut marks, &mut worklist);
                            }
                        }
                    }
                }
            }
        }

        Ok(marks)
    }

    fn mark_env(
        &self,
        key: EnvKey,
        marks: &mut GarbageCollectionMarks,
        worklist: &mut GarbageCollectionWorklist,
    ) {
        if marks.envs.insert(key) {
            worklist.envs.push(key);
        }
    }

    fn mark_cell(
        &self,
        key: CellKey,
        marks: &mut GarbageCollectionMarks,
        worklist: &mut GarbageCollectionWorklist,
    ) {
        if marks.cells.insert(key) {
            worklist.cells.push(key);
        }
    }

    fn mark_frame_roots(
        &self,
        frame: &Frame,
        marks: &mut GarbageCollectionMarks,
        worklist: &mut GarbageCollectionWorklist,
    ) {
        self.mark_env(frame.env, marks, worklist);
        for env in &frame.scope_stack {
            self.mark_env(*env, marks, worklist);
        }
        for value in &frame.stack {
            self.mark_value(value, marks, worklist);
        }
        if let Some(value) = &frame.pending_exception {
            self.mark_value(value, marks, worklist);
        }
        for handler in &frame.handlers {
            self.mark_env(handler.env, marks, worklist);
        }
        for completion in &frame.pending_completions {
            match completion {
                CompletionRecord::Jump { .. } => {}
                CompletionRecord::Return(value) | CompletionRecord::Throw(value) => {
                    self.mark_value(value, marks, worklist);
                }
            }
        }
        if let Some(async_promise) = frame.async_promise {
            self.mark_promise(async_promise, marks, worklist);
        }
    }

    fn mark_value(
        &self,
        value: &Value,
        marks: &mut GarbageCollectionMarks,
        worklist: &mut GarbageCollectionWorklist,
    ) {
        match value {
            Value::Object(key) => {
                if marks.objects.insert(*key) {
                    worklist.objects.push(*key);
                }
            }
            Value::Array(key) => {
                if marks.arrays.insert(*key) {
                    worklist.arrays.push(*key);
                }
            }
            Value::Map(key) => {
                if marks.maps.insert(*key) {
                    worklist.maps.push(*key);
                }
            }
            Value::Set(key) => {
                if marks.sets.insert(*key) {
                    worklist.sets.push(*key);
                }
            }
            Value::Iterator(key) => {
                if marks.iterators.insert(*key) {
                    worklist.iterators.push(*key);
                }
            }
            Value::Closure(key) => {
                if marks.closures.insert(*key) {
                    worklist.closures.push(*key);
                }
            }
            Value::Promise(key) => self.mark_promise(*key, marks, worklist),
            Value::Undefined
            | Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::String(_)
            | Value::BuiltinFunction(_)
            | Value::HostFunction(_) => {}
        }
    }

    fn sweep_unreachable_envs(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .envs
            .keys()
            .filter(|key| !marks.envs.contains(key))
            .collect();
        for key in dead {
            self.envs.remove(key);
        }
    }

    fn sweep_unreachable_cells(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .cells
            .keys()
            .filter(|key| !marks.cells.contains(key))
            .collect();
        for key in dead {
            self.cells.remove(key);
        }
    }

    fn sweep_unreachable_objects(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .objects
            .keys()
            .filter(|key| !marks.objects.contains(key))
            .collect();
        for key in dead {
            self.objects.remove(key);
        }
    }

    fn sweep_unreachable_arrays(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .arrays
            .keys()
            .filter(|key| !marks.arrays.contains(key))
            .collect();
        for key in dead {
            self.arrays.remove(key);
        }
    }

    fn sweep_unreachable_maps(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .maps
            .keys()
            .filter(|key| !marks.maps.contains(key))
            .collect();
        for key in dead {
            self.maps.remove(key);
        }
    }

    fn sweep_unreachable_sets(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .sets
            .keys()
            .filter(|key| !marks.sets.contains(key))
            .collect();
        for key in dead {
            self.sets.remove(key);
        }
    }

    fn sweep_unreachable_iterators(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .iterators
            .keys()
            .filter(|key| !marks.iterators.contains(key))
            .collect();
        for key in dead {
            self.iterators.remove(key);
        }
    }

    fn sweep_unreachable_closures(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .closures
            .keys()
            .filter(|key| !marks.closures.contains(key))
            .collect();
        for key in dead {
            self.closures.remove(key);
        }
    }

    fn sweep_unreachable_promises(&mut self, marks: &GarbageCollectionMarks) {
        let dead: Vec<_> = self
            .promises
            .keys()
            .filter(|key| !marks.promises.contains(key))
            .collect();
        for key in dead {
            self.promises.remove(key);
        }
    }

    fn recompute_accounting_totals(&mut self) -> Result<(usize, usize), String> {
        let mut heap_bytes_used = 0usize;
        let mut allocation_count = 0usize;

        for env in self.envs.values_mut() {
            env.accounted_bytes = measure_env_bytes(env);
            heap_bytes_used = heap_bytes_used
                .checked_add(env.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for cell in self.cells.values_mut() {
            cell.accounted_bytes = measure_cell_bytes(cell);
            heap_bytes_used = heap_bytes_used
                .checked_add(cell.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for object in self.objects.values_mut() {
            object.accounted_bytes = measure_object_bytes(object);
            heap_bytes_used = heap_bytes_used
                .checked_add(object.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for array in self.arrays.values_mut() {
            array.accounted_bytes = measure_array_bytes(array);
            heap_bytes_used = heap_bytes_used
                .checked_add(array.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for map in self.maps.values_mut() {
            map.accounted_bytes = measure_map_bytes(map);
            heap_bytes_used = heap_bytes_used
                .checked_add(map.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for set in self.sets.values_mut() {
            set.accounted_bytes = measure_set_bytes(set);
            heap_bytes_used = heap_bytes_used
                .checked_add(set.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for iterator in self.iterators.values_mut() {
            iterator.accounted_bytes = measure_iterator_bytes(iterator);
            heap_bytes_used = heap_bytes_used
                .checked_add(iterator.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for closure in self.closures.values_mut() {
            closure.accounted_bytes = measure_closure_bytes(closure);
            heap_bytes_used = heap_bytes_used
                .checked_add(closure.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }
        for promise in self.promises.values_mut() {
            promise.accounted_bytes = measure_promise_bytes(promise);
            heap_bytes_used = heap_bytes_used
                .checked_add(promise.accounted_bytes)
                .ok_or_else(|| "heap accounting overflow".to_string())?;
            allocation_count += 1;
        }

        Ok((heap_bytes_used, allocation_count))
    }

    fn mark_promise(
        &self,
        key: PromiseKey,
        marks: &mut GarbageCollectionMarks,
        worklist: &mut GarbageCollectionWorklist,
    ) {
        if marks.promises.insert(key) {
            worklist.promises.push(key);
        }
    }

    fn check_call_depth(&self) -> JsliteResult<()> {
        if self.frames.len() >= self.limits.call_depth_limit {
            return Err(limit_error("call depth limit exceeded"));
        }
        Ok(())
    }

    fn push_frame(
        &mut self,
        function_id: usize,
        env: EnvKey,
        args: &[Value],
        this_value: Value,
        async_promise: Option<PromiseKey>,
    ) -> JsliteResult<()> {
        self.check_call_depth()?;
        let (params, rest) = self
            .program
            .functions
            .get(function_id)
            .map(|function| (function.params.clone(), function.rest.clone()))
            .ok_or_else(|| JsliteError::runtime("function not found"))?;
        let this_cell = self.insert_cell(this_value, true, true)?;
        self.envs
            .get_mut(env)
            .ok_or_else(|| JsliteError::runtime("environment missing"))?
            .bindings
            .insert("this".to_string(), this_cell);
        self.refresh_env_accounting(env)?;
        for pattern in &params {
            for (name, _) in pattern_bindings(pattern) {
                self.declare_name(env, name, true)?;
            }
        }
        for (index, pattern) in params.iter().enumerate() {
            let arg = args.get(index).cloned().unwrap_or(Value::Undefined);
            self.initialize_pattern(env, pattern, arg)?;
        }
        if let Some(rest) = &rest {
            for (name, _) in pattern_bindings(rest) {
                self.declare_name(env, name, true)?;
            }
            let rest_array = self.insert_array(
                args.iter().skip(params.len()).cloned().collect(),
                IndexMap::new(),
            )?;
            self.initialize_pattern(env, rest, Value::Array(rest_array))?;
        }
        self.frames.push(Frame {
            function_id,
            ip: 0,
            env,
            scope_stack: Vec::new(),
            stack: Vec::new(),
            handlers: Vec::new(),
            pending_exception: None,
            pending_completions: Vec::new(),
            active_finally: Vec::new(),
            async_promise,
        });
        Ok(())
    }

    fn call_callable(
        &mut self,
        callee: Value,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<RunState> {
        match callee {
            Value::Closure(closure) => {
                let closure = self
                    .closures
                    .get(closure)
                    .cloned()
                    .ok_or_else(|| JsliteError::runtime("closure not found"))?;
                let env = self.new_env(Some(closure.env))?;
                let (is_async, is_arrow) = self
                    .program
                    .functions
                    .get(closure.function_id)
                    .map(|function| (function.is_async, function.is_arrow))
                    .ok_or_else(|| JsliteError::runtime("function not found"))?;
                let frame_this = if is_arrow {
                    Value::Undefined
                } else {
                    this_value
                };
                if is_async {
                    let promise = self.insert_promise(PromiseState::Pending)?;
                    self.push_frame(closure.function_id, env, args, frame_this, Some(promise))?;
                    Ok(RunState::StartedAsync(Value::Promise(promise)))
                } else {
                    self.push_frame(closure.function_id, env, args, frame_this, None)?;
                    Ok(RunState::PushedFrame)
                }
            }
            Value::BuiltinFunction(function) => Ok(RunState::Completed(
                self.call_builtin(function, this_value, args)?,
            )),
            Value::HostFunction(capability) => {
                let resume_behavior = resume_behavior_for_capability(&capability);
                let args = args
                    .iter()
                    .cloned()
                    .map(|value| self.value_to_structured(value))
                    .collect::<JsliteResult<Vec<_>>>()?;
                if self.current_async_boundary_index().is_some() {
                    let outstanding = self.pending_host_calls.len()
                        + usize::from(self.suspended_host_call.is_some());
                    if outstanding >= self.limits.max_outstanding_host_calls {
                        return Err(limit_error("outstanding host-call limit exhausted"));
                    }
                    let promise = self.insert_promise(PromiseState::Pending)?;
                    self.pending_host_calls.push_back(PendingHostCall {
                        capability,
                        args,
                        promise,
                        resume_behavior,
                        traceback: self.traceback_snapshots(),
                    });
                    Ok(RunState::Completed(Value::Promise(promise)))
                } else {
                    Ok(RunState::Suspended {
                        resume_behavior,
                        capability,
                        args,
                    })
                }
            }
            _ => Err(JsliteError::runtime("value is not callable")),
        }
    }

    fn construct(&mut self, callee: Value, args: &[Value]) -> JsliteResult<Value> {
        match callee {
            Value::BuiltinFunction(
                BuiltinFunction::ArrayCtor
                | BuiltinFunction::ObjectCtor
                | BuiltinFunction::MapCtor
                | BuiltinFunction::SetCtor
                | BuiltinFunction::PromiseCtor
                | BuiltinFunction::ErrorCtor
                | BuiltinFunction::TypeErrorCtor
                | BuiltinFunction::ReferenceErrorCtor
                | BuiltinFunction::RangeErrorCtor
                | BuiltinFunction::NumberCtor
                | BuiltinFunction::StringCtor
                | BuiltinFunction::BooleanCtor,
            ) => match callee {
                Value::BuiltinFunction(BuiltinFunction::MapCtor) => self.construct_map(args),
                Value::BuiltinFunction(BuiltinFunction::SetCtor) => self.construct_set(args),
                Value::BuiltinFunction(kind) => self.call_builtin(kind, Value::Undefined, args),
                _ => unreachable!(),
            },
            _ => Err(JsliteError::runtime(
                "only conservative built-in constructors are supported in v1",
            )),
        }
    }

    fn call_builtin(
        &mut self,
        function: BuiltinFunction,
        this_value: Value,
        args: &[Value],
    ) -> JsliteResult<Value> {
        match function {
            BuiltinFunction::ArrayCtor => {
                let array = self.insert_array(args.to_vec(), IndexMap::new())?;
                Ok(Value::Array(array))
            }
            BuiltinFunction::ArrayIsArray => {
                Ok(Value::Bool(matches!(args.first(), Some(Value::Array(_)))))
            }
            BuiltinFunction::ArrayPush => self.call_array_push(this_value, args),
            BuiltinFunction::ArrayPop => self.call_array_pop(this_value),
            BuiltinFunction::ArraySlice => self.call_array_slice(this_value, args),
            BuiltinFunction::ArrayJoin => self.call_array_join(this_value, args),
            BuiltinFunction::ArrayIncludes => self.call_array_includes(this_value, args),
            BuiltinFunction::ArrayIndexOf => self.call_array_index_of(this_value, args),
            BuiltinFunction::ArrayValues => self.call_array_values(this_value),
            BuiltinFunction::ArrayKeys => self.call_array_keys(this_value),
            BuiltinFunction::ArrayEntries => self.call_array_entries(this_value),
            BuiltinFunction::ObjectCtor => {
                if let Some(Value::Object(object)) = args.first() {
                    Ok(Value::Object(*object))
                } else {
                    let object = self.insert_object(IndexMap::new(), ObjectKind::Plain)?;
                    Ok(Value::Object(object))
                }
            }
            BuiltinFunction::ObjectKeys => self.call_object_keys(args),
            BuiltinFunction::ObjectValues => self.call_object_values(args),
            BuiltinFunction::ObjectEntries => self.call_object_entries(args),
            BuiltinFunction::ObjectHasOwn => self.call_object_has_own(args),
            BuiltinFunction::MapCtor => Err(JsliteError::runtime(
                "TypeError: Map constructor must be called with new",
            )),
            BuiltinFunction::MapGet => self.call_map_get(this_value, args),
            BuiltinFunction::MapSet => self.call_map_set(this_value, args),
            BuiltinFunction::MapHas => self.call_map_has(this_value, args),
            BuiltinFunction::MapDelete => self.call_map_delete(this_value, args),
            BuiltinFunction::MapClear => self.call_map_clear(this_value),
            BuiltinFunction::MapEntries => self.call_map_entries(this_value),
            BuiltinFunction::MapKeys => self.call_map_keys(this_value),
            BuiltinFunction::MapValues => self.call_map_values(this_value),
            BuiltinFunction::SetCtor => Err(JsliteError::runtime(
                "TypeError: Set constructor must be called with new",
            )),
            BuiltinFunction::SetAdd => self.call_set_add(this_value, args),
            BuiltinFunction::SetHas => self.call_set_has(this_value, args),
            BuiltinFunction::SetDelete => self.call_set_delete(this_value, args),
            BuiltinFunction::SetClear => self.call_set_clear(this_value),
            BuiltinFunction::SetEntries => self.call_set_entries(this_value),
            BuiltinFunction::SetKeys => self.call_set_keys(this_value),
            BuiltinFunction::SetValues => self.call_set_values(this_value),
            BuiltinFunction::IteratorNext => self.call_iterator_next(this_value),
            BuiltinFunction::PromiseCtor => Err(JsliteError::runtime(
                "Promise construction is not supported in v1; use async functions or Promise.resolve/reject",
            )),
            BuiltinFunction::PromiseResolve => {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                if let Value::Promise(_) = value {
                    Ok(value)
                } else {
                    Ok(Value::Promise(
                        self.insert_promise(PromiseState::Fulfilled(value))?,
                    ))
                }
            }
            BuiltinFunction::PromiseReject => {
                let value = args.first().cloned().unwrap_or(Value::Undefined);
                Ok(Value::Promise(self.insert_promise(
                    PromiseState::Rejected(PromiseRejection {
                        value,
                        span: None,
                        traceback: Vec::new(),
                    }),
                )?))
            }
            BuiltinFunction::PromiseThen => self.call_promise_then(this_value, args),
            BuiltinFunction::PromiseCatch => self.call_promise_catch(this_value, args),
            BuiltinFunction::PromiseFinally => self.call_promise_finally(this_value, args),
            BuiltinFunction::PromiseAll => self.call_promise_all(args),
            BuiltinFunction::PromiseRace => self.call_promise_race(args),
            BuiltinFunction::PromiseAny => self.call_promise_any(args),
            BuiltinFunction::PromiseAllSettled => self.call_promise_all_settled(args),
            BuiltinFunction::ErrorCtor => self.make_error_object("Error", args, None, None),
            BuiltinFunction::TypeErrorCtor => self.make_error_object("TypeError", args, None, None),
            BuiltinFunction::ReferenceErrorCtor => {
                self.make_error_object("ReferenceError", args, None, None)
            }
            BuiltinFunction::RangeErrorCtor => {
                self.make_error_object("RangeError", args, None, None)
            }
            BuiltinFunction::NumberCtor => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?,
            )),
            BuiltinFunction::StringCtor => Ok(Value::String(
                self.to_string(args.first().cloned().unwrap_or(Value::Undefined))?,
            )),
            BuiltinFunction::StringTrim => self.call_string_trim(this_value),
            BuiltinFunction::StringIncludes => self.call_string_includes(this_value, args),
            BuiltinFunction::StringStartsWith => self.call_string_starts_with(this_value, args),
            BuiltinFunction::StringEndsWith => self.call_string_ends_with(this_value, args),
            BuiltinFunction::StringSlice => self.call_string_slice(this_value, args),
            BuiltinFunction::StringSubstring => self.call_string_substring(this_value, args),
            BuiltinFunction::StringToLowerCase => self.call_string_to_lower_case(this_value),
            BuiltinFunction::StringToUpperCase => self.call_string_to_upper_case(this_value),
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
            BuiltinFunction::MathPow => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                    .powf(self.to_number(args.get(1).cloned().unwrap_or(Value::Undefined))?),
            )),
            BuiltinFunction::MathSqrt => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                    .sqrt(),
            )),
            BuiltinFunction::MathTrunc => Ok(Value::Number(
                self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?
                    .trunc(),
            )),
            BuiltinFunction::MathSign => {
                let value = self.to_number(args.first().cloned().unwrap_or(Value::Undefined))?;
                Ok(Value::Number(if value.is_nan() {
                    f64::NAN
                } else if value == 0.0 {
                    value
                } else if value.is_sign_positive() {
                    1.0
                } else {
                    -1.0
                }))
            }
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
        let global_object = self.insert_object(IndexMap::new(), ObjectKind::Global)?;
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
            "Map".to_string(),
            Value::BuiltinFunction(BuiltinFunction::MapCtor),
            false,
        )?;
        self.define_global(
            "Set".to_string(),
            Value::BuiltinFunction(BuiltinFunction::SetCtor),
            false,
        )?;
        self.define_global(
            "Array".to_string(),
            Value::BuiltinFunction(BuiltinFunction::ArrayCtor),
            false,
        )?;
        self.define_global(
            "Promise".to_string(),
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor),
            false,
        )?;
        self.define_global(
            "String".to_string(),
            Value::BuiltinFunction(BuiltinFunction::StringCtor),
            false,
        )?;
        self.define_global(
            "Error".to_string(),
            Value::BuiltinFunction(BuiltinFunction::ErrorCtor),
            false,
        )?;
        self.define_global(
            "TypeError".to_string(),
            Value::BuiltinFunction(BuiltinFunction::TypeErrorCtor),
            false,
        )?;
        self.define_global(
            "ReferenceError".to_string(),
            Value::BuiltinFunction(BuiltinFunction::ReferenceErrorCtor),
            false,
        )?;
        self.define_global(
            "RangeError".to_string(),
            Value::BuiltinFunction(BuiltinFunction::RangeErrorCtor),
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

        let math = self.insert_object(
            IndexMap::from([
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
                (
                    "pow".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathPow),
                ),
                (
                    "sqrt".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathSqrt),
                ),
                (
                    "trunc".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathTrunc),
                ),
                (
                    "sign".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::MathSign),
                ),
            ]),
            ObjectKind::Math,
        )?;
        self.define_global("Math".to_string(), Value::Object(math), false)?;

        let json = self.insert_object(
            IndexMap::from([
                (
                    "stringify".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::JsonStringify),
                ),
                (
                    "parse".to_string(),
                    Value::BuiltinFunction(BuiltinFunction::JsonParse),
                ),
            ]),
            ObjectKind::Json,
        )?;
        self.define_global("JSON".to_string(), Value::Object(json), false)?;

        let console = self.insert_object(IndexMap::new(), ObjectKind::Console)?;
        self.define_global("console".to_string(), Value::Object(console), false)?;
        Ok(())
    }

    fn construct_map(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let map = self.insert_map(Vec::new())?;
        let iterable = args.first().cloned().unwrap_or(Value::Undefined);
        if matches!(iterable, Value::Null | Value::Undefined) {
            return Ok(Value::Map(map));
        }

        let iterator = self.create_iterator(iterable)?;
        loop {
            let (entry, done) = self.iterator_next(iterator.clone())?;
            if done {
                break;
            }
            let items = match entry {
                Value::Array(array) => self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?
                    .elements
                    .clone(),
                _ => {
                    return Err(JsliteError::runtime(
                        "TypeError: Map constructor expects an iterable of [key, value] pairs",
                    ));
                }
            };
            let key = items.first().cloned().unwrap_or(Value::Undefined);
            let value = items.get(1).cloned().unwrap_or(Value::Undefined);
            self.map_set(map, key, value)?;
        }

        Ok(Value::Map(map))
    }

    fn construct_set(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let set = self.insert_set(Vec::new())?;
        let iterable = args.first().cloned().unwrap_or(Value::Undefined);
        if matches!(iterable, Value::Null | Value::Undefined) {
            return Ok(Value::Set(set));
        }

        let iterator = self.create_iterator(iterable)?;
        loop {
            let (value, done) = self.iterator_next(iterator.clone())?;
            if done {
                break;
            }
            self.set_add(set, value)?;
        }

        Ok(Value::Set(set))
    }

    fn array_receiver(&self, value: Value, method: &str) -> JsliteResult<ArrayKey> {
        match value {
            Value::Array(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: Array.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn string_receiver(&self, value: Value, method: &str) -> JsliteResult<String> {
        match value {
            Value::String(value) => Ok(value),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: String.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn call_array_push(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "push")?;
        {
            let elements = &mut self
                .arrays
                .get_mut(array)
                .ok_or_else(|| JsliteError::runtime("array missing"))?
                .elements;
            elements.extend(args.iter().cloned());
        }
        self.refresh_array_accounting(array)?;
        let length = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .len();
        Ok(Value::Number(length as f64))
    }

    fn call_array_pop(&mut self, this_value: Value) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "pop")?;
        let value = self
            .arrays
            .get_mut(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .pop()
            .unwrap_or(Value::Undefined);
        self.refresh_array_accounting(array)?;
        Ok(value)
    }

    fn call_array_slice(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "slice")?;
        let elements = self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements
            .clone();
        let start = normalize_relative_bound(
            self.to_integer(args.first().cloned().unwrap_or(Value::Number(0.0)))?,
            elements.len(),
        );
        let end = normalize_relative_bound(
            match args.get(1) {
                Some(value) => self.to_integer(value.clone())?,
                None => elements.len() as i64,
            },
            elements.len(),
        );
        let end = end.max(start);
        Ok(Value::Array(self.insert_array(
            elements[start..end].to_vec(),
            IndexMap::new(),
        )?))
    }

    fn call_array_join(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "join")?;
        let separator = match args.first() {
            Some(value) => self.to_string(value.clone())?,
            None => ",".to_string(),
        };
        let elements = &self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements;
        let mut parts = Vec::with_capacity(elements.len());
        for value in elements {
            parts.push(match value {
                Value::Undefined | Value::Null => String::new(),
                other => self.to_string(other.clone())?,
            });
        }
        Ok(Value::String(parts.join(&separator)))
    }

    fn call_array_includes(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "includes")?;
        let search = args.first().cloned().unwrap_or(Value::Undefined);
        let elements = &self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements;
        let start = normalize_search_index(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?,
            elements.len(),
        );
        Ok(Value::Bool(
            elements
                .iter()
                .skip(start)
                .any(|value| same_value_zero(value, &search)),
        ))
    }

    fn call_array_index_of(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "indexOf")?;
        let search = args.first().cloned().unwrap_or(Value::Undefined);
        let elements = &self
            .arrays
            .get(array)
            .ok_or_else(|| JsliteError::runtime("array missing"))?
            .elements;
        let start = normalize_search_index(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?,
            elements.len(),
        );
        let index = elements
            .iter()
            .enumerate()
            .skip(start)
            .find(|(_, value)| strict_equal(value, &search))
            .map(|(index, _)| index as f64)
            .unwrap_or(-1.0);
        Ok(Value::Number(index))
    }

    fn call_array_values(&mut self, this_value: Value) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "values")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::Array(ArrayIteratorState {
                array,
                next_index: 0,
            }),
        )?))
    }

    fn call_array_keys(&mut self, this_value: Value) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "keys")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::ArrayKeys(ArrayIteratorState {
                array,
                next_index: 0,
            }),
        )?))
    }

    fn call_array_entries(&mut self, this_value: Value) -> JsliteResult<Value> {
        let array = self.array_receiver(this_value, "entries")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::ArrayEntries(ArrayIteratorState {
                array,
                next_index: 0,
            }),
        )?))
    }

    fn enumerable_keys(&self, value: Value) -> JsliteResult<Vec<String>> {
        match value {
            Value::Object(object) => {
                let mut keys = self
                    .objects
                    .get(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?
                    .properties
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>();
                keys.sort();
                Ok(keys)
            }
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                let mut keys = (0..array.elements.len())
                    .map(|index| index.to_string())
                    .collect::<Vec<_>>();
                let mut extra = array.properties.keys().cloned().collect::<Vec<_>>();
                extra.sort();
                keys.extend(extra);
                Ok(keys)
            }
            _ => Err(JsliteError::runtime(
                "TypeError: Object helpers currently only support plain objects and arrays",
            )),
        }
    }

    fn enumerable_value(&self, target: Value, key: &str) -> JsliteResult<Value> {
        match target {
            Value::Object(object) => self
                .objects
                .get(object)
                .and_then(|object| object.properties.get(key))
                .cloned()
                .ok_or_else(|| JsliteError::runtime("object property missing")),
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                if let Ok(index) = key.parse::<usize>() {
                    Ok(array
                        .elements
                        .get(index)
                        .cloned()
                        .unwrap_or(Value::Undefined))
                } else {
                    array
                        .properties
                        .get(key)
                        .cloned()
                        .ok_or_else(|| JsliteError::runtime("array property missing"))
                }
            }
            _ => Err(JsliteError::runtime(
                "TypeError: Object helpers currently only support plain objects and arrays",
            )),
        }
    }

    fn call_object_keys(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = args.first().cloned().unwrap_or(Value::Undefined);
        let keys = self
            .enumerable_keys(target)?
            .into_iter()
            .map(Value::String)
            .collect();
        Ok(Value::Array(self.insert_array(keys, IndexMap::new())?))
    }

    fn call_object_values(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = args.first().cloned().unwrap_or(Value::Undefined);
        let keys = self.enumerable_keys(target.clone())?;
        let mut values = Vec::with_capacity(keys.len());
        for key in keys {
            values.push(self.enumerable_value(target.clone(), &key)?);
        }
        Ok(Value::Array(self.insert_array(values, IndexMap::new())?))
    }

    fn call_object_entries(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = args.first().cloned().unwrap_or(Value::Undefined);
        let keys = self.enumerable_keys(target.clone())?;
        let mut entries = Vec::with_capacity(keys.len());
        for key in keys {
            let pair = self.insert_array(
                vec![
                    Value::String(key.clone()),
                    self.enumerable_value(target.clone(), &key)?,
                ],
                IndexMap::new(),
            )?;
            entries.push(Value::Array(pair));
        }
        Ok(Value::Array(self.insert_array(entries, IndexMap::new())?))
    }

    fn call_object_has_own(&self, args: &[Value]) -> JsliteResult<Value> {
        let target = args.first().cloned().unwrap_or(Value::Undefined);
        let key = self.to_property_key(args.get(1).cloned().unwrap_or(Value::Undefined))?;
        let has_key = match target {
            Value::Object(object) => self
                .objects
                .get(object)
                .ok_or_else(|| JsliteError::runtime("object missing"))?
                .properties
                .contains_key(&key),
            Value::Array(array) => {
                let array = self
                    .arrays
                    .get(array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?;
                key.parse::<usize>()
                    .ok()
                    .is_some_and(|index| index < array.elements.len())
                    || array.properties.contains_key(&key)
            }
            _ => {
                return Err(JsliteError::runtime(
                    "TypeError: Object helpers currently only support plain objects and arrays",
                ));
            }
        };
        Ok(Value::Bool(has_key))
    }

    fn call_string_trim(&self, this_value: Value) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "trim")?;
        Ok(Value::String(value.trim().to_string()))
    }

    fn call_string_includes(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "includes")?;
        let chars = value.chars().collect::<Vec<_>>();
        let needle = self
            .to_string(args.first().cloned().unwrap_or(Value::Undefined))?
            .chars()
            .collect::<Vec<_>>();
        let position = clamp_index(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?,
            chars.len(),
        );
        let haystack = chars[position..].iter().collect::<String>();
        Ok(Value::Bool(
            haystack.contains(&needle.iter().collect::<String>()),
        ))
    }

    fn call_string_starts_with(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "startsWith")?;
        let chars = value.chars().collect::<Vec<_>>();
        let needle = self
            .to_string(args.first().cloned().unwrap_or(Value::Undefined))?
            .chars()
            .collect::<Vec<_>>();
        let position = clamp_index(
            self.to_integer(args.get(1).cloned().unwrap_or(Value::Number(0.0)))?,
            chars.len(),
        );
        Ok(Value::Bool(chars[position..].starts_with(&needle)))
    }

    fn call_string_ends_with(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "endsWith")?;
        let chars = value.chars().collect::<Vec<_>>();
        let needle = self
            .to_string(args.first().cloned().unwrap_or(Value::Undefined))?
            .chars()
            .collect::<Vec<_>>();
        let end = match args.get(1) {
            Some(value) => clamp_index(self.to_integer(value.clone())?, chars.len()),
            None => chars.len(),
        };
        Ok(Value::Bool(chars[..end].ends_with(&needle)))
    }

    fn call_string_slice(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "slice")?;
        let chars = value.chars().collect::<Vec<_>>();
        let start = normalize_relative_bound(
            self.to_integer(args.first().cloned().unwrap_or(Value::Number(0.0)))?,
            chars.len(),
        );
        let end = normalize_relative_bound(
            match args.get(1) {
                Some(value) => self.to_integer(value.clone())?,
                None => chars.len() as i64,
            },
            chars.len(),
        );
        let end = end.max(start);
        Ok(Value::String(chars[start..end].iter().collect()))
    }

    fn call_string_substring(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "substring")?;
        let chars = value.chars().collect::<Vec<_>>();
        let start = clamp_index(
            self.to_integer(args.first().cloned().unwrap_or(Value::Number(0.0)))?,
            chars.len(),
        );
        let end = match args.get(1) {
            Some(value) => clamp_index(self.to_integer(value.clone())?, chars.len()),
            None => chars.len(),
        };
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        Ok(Value::String(chars[start..end].iter().collect()))
    }

    fn call_string_to_lower_case(&self, this_value: Value) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "toLowerCase")?;
        Ok(Value::String(value.to_lowercase()))
    }

    fn call_string_to_upper_case(&self, this_value: Value) -> JsliteResult<Value> {
        let value = self.string_receiver(this_value, "toUpperCase")?;
        Ok(Value::String(value.to_uppercase()))
    }

    fn map_receiver(&self, value: Value, method: &str) -> JsliteResult<MapKey> {
        match value {
            Value::Map(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: Map.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn set_receiver(&self, value: Value, method: &str) -> JsliteResult<SetKey> {
        match value {
            Value::Set(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: Set.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn iterator_receiver(&self, value: Value, method: &str) -> JsliteResult<IteratorKey> {
        match value {
            Value::Iterator(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: iterator.{method} called on incompatible receiver",
            ))),
        }
    }

    fn call_map_get(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let map = self.map_receiver(this_value, "get")?;
        Ok(self
            .map_get(map, &key)?
            .map(|entry| entry.value)
            .unwrap_or(Value::Undefined))
    }

    fn call_map_set(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "set")?;
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let value = args.get(1).cloned().unwrap_or(Value::Undefined);
        self.map_set(map, key, value)?;
        Ok(Value::Map(map))
    }

    fn call_map_has(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let map = self.map_receiver(this_value, "has")?;
        Ok(Value::Bool(self.map_get(map, &key)?.is_some()))
    }

    fn call_map_delete(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let key = args.first().cloned().unwrap_or(Value::Undefined);
        let map = self.map_receiver(this_value, "delete")?;
        Ok(Value::Bool(self.map_delete(map, &key)?))
    }

    fn call_map_clear(&mut self, this_value: Value) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "clear")?;
        self.map_clear(map)?;
        Ok(Value::Undefined)
    }

    fn call_map_entries(&mut self, this_value: Value) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "entries")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapEntries(MapIteratorState { map, next_index: 0 }),
        )?))
    }

    fn call_map_keys(&mut self, this_value: Value) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "keys")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapKeys(MapIteratorState { map, next_index: 0 }),
        )?))
    }

    fn call_map_values(&mut self, this_value: Value) -> JsliteResult<Value> {
        let map = self.map_receiver(this_value, "values")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::MapValues(MapIteratorState { map, next_index: 0 }),
        )?))
    }

    fn call_set_add(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "add")?;
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        self.set_add(set, value)?;
        Ok(Value::Set(set))
    }

    fn call_set_has(&self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        let set = self.set_receiver(this_value, "has")?;
        Ok(Value::Bool(self.set_contains(set, &value)?))
    }

    fn call_set_delete(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let value = args.first().cloned().unwrap_or(Value::Undefined);
        let set = self.set_receiver(this_value, "delete")?;
        Ok(Value::Bool(self.set_delete(set, &value)?))
    }

    fn call_set_clear(&mut self, this_value: Value) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "clear")?;
        self.set_clear(set)?;
        Ok(Value::Undefined)
    }

    fn call_set_entries(&mut self, this_value: Value) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "entries")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetEntries(SetIteratorState { set, next_index: 0 }),
        )?))
    }

    fn call_set_keys(&mut self, this_value: Value) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "keys")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetValues(SetIteratorState { set, next_index: 0 }),
        )?))
    }

    fn call_set_values(&mut self, this_value: Value) -> JsliteResult<Value> {
        let set = self.set_receiver(this_value, "values")?;
        Ok(Value::Iterator(self.insert_iterator(
            IteratorState::SetValues(SetIteratorState { set, next_index: 0 }),
        )?))
    }

    fn call_iterator_next(&mut self, this_value: Value) -> JsliteResult<Value> {
        let iterator = self.iterator_receiver(this_value, "next")?;
        let (value, done) = self.iterator_next(Value::Iterator(iterator))?;
        let result = self.insert_object(
            IndexMap::from([
                ("value".to_string(), value),
                ("done".to_string(), Value::Bool(done)),
            ]),
            ObjectKind::Plain,
        )?;
        Ok(Value::Object(result))
    }

    fn promise_receiver(&self, value: Value, method: &str) -> JsliteResult<PromiseKey> {
        match value {
            Value::Promise(key) => Ok(key),
            _ => Err(JsliteError::runtime(format!(
                "TypeError: Promise.prototype.{method} called on incompatible receiver",
            ))),
        }
    }

    fn collect_iterable_values(&mut self, iterable: Value) -> JsliteResult<Vec<Value>> {
        let iterator = self.create_iterator(iterable)?;
        let mut values = Vec::new();
        loop {
            let (value, done) = self.iterator_next(iterator.clone())?;
            if done {
                break;
            }
            values.push(value);
        }
        Ok(values)
    }

    fn call_promise_then(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let promise = self.promise_receiver(this_value, "then")?;
        let target = self.insert_promise(PromiseState::Pending)?;
        let on_fulfilled = args.first().cloned().filter(is_callable);
        let on_rejected = args.get(1).cloned().filter(is_callable);
        self.attach_promise_reaction(
            promise,
            PromiseReaction::Then {
                target,
                on_fulfilled,
                on_rejected,
            },
        )?;
        Ok(Value::Promise(target))
    }

    fn call_promise_catch(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let promise = self.promise_receiver(this_value, "catch")?;
        let target = self.insert_promise(PromiseState::Pending)?;
        let on_rejected = args.first().cloned().filter(is_callable);
        self.attach_promise_reaction(
            promise,
            PromiseReaction::Then {
                target,
                on_fulfilled: None,
                on_rejected,
            },
        )?;
        Ok(Value::Promise(target))
    }

    fn call_promise_finally(&mut self, this_value: Value, args: &[Value]) -> JsliteResult<Value> {
        let promise = self.promise_receiver(this_value, "finally")?;
        let target = self.insert_promise(PromiseState::Pending)?;
        let callback = args.first().cloned().filter(is_callable);
        self.attach_promise_reaction(promise, PromiseReaction::Finally { target, callback })?;
        Ok(Value::Promise(target))
    }

    fn call_promise_all(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = self.insert_promise(PromiseState::Pending)?;
        let values =
            self.collect_iterable_values(args.first().cloned().unwrap_or(Value::Undefined))?;
        if values.is_empty() {
            let array = Value::Array(self.insert_array(Vec::new(), IndexMap::new())?);
            self.resolve_promise(target, array)?;
            return Ok(Value::Promise(target));
        }
        self.promises
            .get_mut(target)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .driver = Some(PromiseDriver::All {
            remaining: values.len(),
            values: vec![None; values.len()],
        });
        self.refresh_promise_accounting(target)?;
        for (index, value) in values.into_iter().enumerate() {
            let promise = self.coerce_to_promise(value)?;
            self.attach_promise_reaction(
                promise,
                PromiseReaction::Combinator {
                    target,
                    index,
                    kind: PromiseCombinatorKind::All,
                },
            )?;
        }
        Ok(Value::Promise(target))
    }

    fn call_promise_race(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = self.insert_promise(PromiseState::Pending)?;
        for value in
            self.collect_iterable_values(args.first().cloned().unwrap_or(Value::Undefined))?
        {
            let promise = self.coerce_to_promise(value)?;
            self.attach_promise_reaction(
                promise,
                PromiseReaction::Combinator {
                    target,
                    index: 0,
                    kind: PromiseCombinatorKind::Race,
                },
            )?;
        }
        Ok(Value::Promise(target))
    }

    fn call_promise_any(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = self.insert_promise(PromiseState::Pending)?;
        let values =
            self.collect_iterable_values(args.first().cloned().unwrap_or(Value::Undefined))?;
        if values.is_empty() {
            let error = self.make_aggregate_error(Vec::new())?;
            self.reject_promise(
                target,
                PromiseRejection {
                    value: error,
                    span: None,
                    traceback: self.traceback_snapshots(),
                },
            )?;
            return Ok(Value::Promise(target));
        }
        self.promises
            .get_mut(target)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .driver = Some(PromiseDriver::Any {
            remaining: values.len(),
            reasons: vec![None; values.len()],
        });
        self.refresh_promise_accounting(target)?;
        for (index, value) in values.into_iter().enumerate() {
            let promise = self.coerce_to_promise(value)?;
            self.attach_promise_reaction(
                promise,
                PromiseReaction::Combinator {
                    target,
                    index,
                    kind: PromiseCombinatorKind::Any,
                },
            )?;
        }
        Ok(Value::Promise(target))
    }

    fn call_promise_all_settled(&mut self, args: &[Value]) -> JsliteResult<Value> {
        let target = self.insert_promise(PromiseState::Pending)?;
        let values =
            self.collect_iterable_values(args.first().cloned().unwrap_or(Value::Undefined))?;
        if values.is_empty() {
            let array = Value::Array(self.insert_array(Vec::new(), IndexMap::new())?);
            self.resolve_promise(target, array)?;
            return Ok(Value::Promise(target));
        }
        self.promises
            .get_mut(target)
            .ok_or_else(|| JsliteError::runtime("promise missing"))?
            .driver = Some(PromiseDriver::AllSettled {
            remaining: values.len(),
            results: vec![None; values.len()],
        });
        self.refresh_promise_accounting(target)?;
        for (index, value) in values.into_iter().enumerate() {
            let promise = self.coerce_to_promise(value)?;
            self.attach_promise_reaction(
                promise,
                PromiseReaction::Combinator {
                    target,
                    index,
                    kind: PromiseCombinatorKind::AllSettled,
                },
            )?;
        }
        Ok(Value::Promise(target))
    }

    fn map_get(&self, map: MapKey, key: &Value) -> JsliteResult<Option<MapEntry>> {
        let normalized = canonicalize_collection_key(key.clone());
        Ok(self
            .maps
            .get(map)
            .ok_or_else(|| JsliteError::runtime("map missing"))?
            .entries
            .iter()
            .find(|entry| same_value_zero(&entry.key, &normalized))
            .cloned())
    }

    fn map_set(&mut self, map: MapKey, key: Value, value: Value) -> JsliteResult<()> {
        let key = canonicalize_collection_key(key);
        {
            let entries = &mut self
                .maps
                .get_mut(map)
                .ok_or_else(|| JsliteError::runtime("map missing"))?
                .entries;
            if let Some(entry) = entries
                .iter_mut()
                .find(|entry| same_value_zero(&entry.key, &key))
            {
                entry.value = value;
            } else {
                entries.push(MapEntry { key, value });
            }
        }
        self.refresh_map_accounting(map)
    }

    fn map_delete(&mut self, map: MapKey, key: &Value) -> JsliteResult<bool> {
        let normalized = canonicalize_collection_key(key.clone());
        let removed = {
            let entries = &mut self
                .maps
                .get_mut(map)
                .ok_or_else(|| JsliteError::runtime("map missing"))?
                .entries;
            if let Some(index) = entries
                .iter()
                .position(|entry| same_value_zero(&entry.key, &normalized))
            {
                entries.remove(index);
                true
            } else {
                false
            }
        };
        if removed {
            self.refresh_map_accounting(map)?;
        }
        Ok(removed)
    }

    fn map_clear(&mut self, map: MapKey) -> JsliteResult<()> {
        self.maps
            .get_mut(map)
            .ok_or_else(|| JsliteError::runtime("map missing"))?
            .entries
            .clear();
        self.refresh_map_accounting(map)
    }

    fn set_add(&mut self, set: SetKey, value: Value) -> JsliteResult<()> {
        let value = canonicalize_collection_key(value);
        {
            let entries = &mut self
                .sets
                .get_mut(set)
                .ok_or_else(|| JsliteError::runtime("set missing"))?
                .entries;
            if !entries.iter().any(|entry| same_value_zero(entry, &value)) {
                entries.push(value);
            }
        }
        self.refresh_set_accounting(set)
    }

    fn set_contains(&self, set: SetKey, value: &Value) -> JsliteResult<bool> {
        let normalized = canonicalize_collection_key(value.clone());
        Ok(self
            .sets
            .get(set)
            .ok_or_else(|| JsliteError::runtime("set missing"))?
            .entries
            .iter()
            .any(|entry| same_value_zero(entry, &normalized)))
    }

    fn set_delete(&mut self, set: SetKey, value: &Value) -> JsliteResult<bool> {
        let normalized = canonicalize_collection_key(value.clone());
        let removed = {
            let entries = &mut self
                .sets
                .get_mut(set)
                .ok_or_else(|| JsliteError::runtime("set missing"))?
                .entries;
            if let Some(index) = entries
                .iter()
                .position(|entry| same_value_zero(entry, &normalized))
            {
                entries.remove(index);
                true
            } else {
                false
            }
        };
        if removed {
            self.refresh_set_accounting(set)?;
        }
        Ok(removed)
    }

    fn set_clear(&mut self, set: SetKey) -> JsliteResult<()> {
        self.sets
            .get_mut(set)
            .ok_or_else(|| JsliteError::runtime("set missing"))?
            .entries
            .clear();
        self.refresh_set_accounting(set)
    }

    fn new_env(&mut self, parent: Option<EnvKey>) -> JsliteResult<EnvKey> {
        self.insert_env(parent)
    }

    fn define_global(&mut self, name: String, value: Value, mutable: bool) -> JsliteResult<()> {
        let cell = self.insert_cell(value, mutable, true)?;
        self.envs
            .get_mut(self.globals)
            .ok_or_else(|| JsliteError::runtime("missing globals environment"))?
            .bindings
            .insert(name, cell);
        self.refresh_env_accounting(self.globals)?;
        Ok(())
    }

    fn declare_name(&mut self, env: EnvKey, name: String, mutable: bool) -> JsliteResult<()> {
        if self
            .envs
            .get(env)
            .ok_or_else(|| JsliteError::runtime("environment missing"))?
            .bindings
            .contains_key(&name)
        {
            return Ok(());
        }
        let cell = self.insert_cell(Value::Undefined, mutable, false)?;
        self.envs
            .get_mut(env)
            .ok_or_else(|| JsliteError::runtime("environment missing"))?
            .bindings
            .insert(name, cell);
        self.refresh_env_accounting(env)?;
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
        {
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
        }
        self.refresh_cell_accounting(cell_key)?;
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
        let mut was_initialized = false;
        {
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
                was_initialized = true;
            } else {
                cell.value = value;
                cell.initialized = true;
            }
        }
        self.refresh_cell_accounting(cell_key)?;
        if was_initialized {
            return Ok(());
        }
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
                            rest: None,
                            code: Vec::new(),
                            is_async: false,
                            is_arrow: false,
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
                    let array = self.insert_array(
                        items.into_iter().skip(elements.len()).collect(),
                        IndexMap::new(),
                    )?;
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
                    let rest = self.insert_object(rest_object, ObjectKind::Plain)?;
                    self.initialize_pattern(env, rest_pattern, Value::Object(rest))?;
                }
                Ok(())
            }
        }
    }

    fn create_iterator(&mut self, iterable: Value) -> JsliteResult<Value> {
        match iterable {
            Value::Array(array) => Ok(Value::Iterator(self.insert_iterator(
                IteratorState::Array(ArrayIteratorState {
                    array,
                    next_index: 0,
                }),
            )?)),
            Value::String(value) => Ok(Value::Iterator(self.insert_iterator(
                IteratorState::String(StringIteratorState {
                    value,
                    next_index: 0,
                }),
            )?)),
            Value::Map(map) => Ok(Value::Iterator(self.insert_iterator(
                IteratorState::MapEntries(MapIteratorState { map, next_index: 0 }),
            )?)),
            Value::Set(set) => Ok(Value::Iterator(self.insert_iterator(
                IteratorState::SetValues(SetIteratorState { set, next_index: 0 }),
            )?)),
            Value::Iterator(iterator) => Ok(Value::Iterator(iterator)),
            _ => Err(JsliteError::runtime(
                "TypeError: value is not iterable in the supported surface",
            )),
        }
    }

    fn iterator_next(&mut self, iterator: Value) -> JsliteResult<(Value, bool)> {
        let key = match iterator {
            Value::Iterator(key) => key,
            _ => return Err(JsliteError::runtime("TypeError: value is not an iterator")),
        };
        let state = self
            .iterators
            .get(key)
            .ok_or_else(|| JsliteError::runtime("iterator missing"))?
            .state
            .clone();

        let value = match state {
            IteratorState::Array(state) => self
                .arrays
                .get(state.array)
                .ok_or_else(|| JsliteError::runtime("array missing"))?
                .elements
                .get(state.next_index)
                .cloned(),
            IteratorState::ArrayKeys(state) => self
                .arrays
                .get(state.array)
                .ok_or_else(|| JsliteError::runtime("array missing"))?
                .elements
                .get(state.next_index)
                .map(|_| Value::Number(state.next_index as f64)),
            IteratorState::ArrayEntries(state) => {
                let value = self
                    .arrays
                    .get(state.array)
                    .ok_or_else(|| JsliteError::runtime("array missing"))?
                    .elements
                    .get(state.next_index)
                    .cloned();
                match value {
                    Some(value) => Some(Value::Array(self.insert_array(
                        vec![Value::Number(state.next_index as f64), value],
                        IndexMap::new(),
                    )?)),
                    None => None,
                }
            }
            IteratorState::String(state) => {
                let chars = state.value.chars().collect::<Vec<_>>();
                chars
                    .get(state.next_index)
                    .map(|ch| Value::String(ch.to_string()))
            }
            IteratorState::MapEntries(state) => {
                let entry = self
                    .maps
                    .get(state.map)
                    .ok_or_else(|| JsliteError::runtime("map missing"))?
                    .entries
                    .get(state.next_index)
                    .cloned();
                match entry {
                    Some(entry) => Some(Value::Array(
                        self.insert_array(vec![entry.key, entry.value], IndexMap::new())?,
                    )),
                    None => None,
                }
            }
            IteratorState::MapKeys(state) => self
                .maps
                .get(state.map)
                .ok_or_else(|| JsliteError::runtime("map missing"))?
                .entries
                .get(state.next_index)
                .map(|entry| entry.key.clone()),
            IteratorState::MapValues(state) => self
                .maps
                .get(state.map)
                .ok_or_else(|| JsliteError::runtime("map missing"))?
                .entries
                .get(state.next_index)
                .map(|entry| entry.value.clone()),
            IteratorState::SetEntries(state) => self
                .sets
                .get(state.set)
                .ok_or_else(|| JsliteError::runtime("set missing"))?
                .entries
                .get(state.next_index)
                .cloned()
                .map(|value| {
                    self.insert_array(vec![value.clone(), value], IndexMap::new())
                        .map(Value::Array)
                })
                .transpose()?,
            IteratorState::SetValues(state) => self
                .sets
                .get(state.set)
                .ok_or_else(|| JsliteError::runtime("set missing"))?
                .entries
                .get(state.next_index)
                .cloned(),
        };

        if value.is_some() {
            if let Some(iterator) = self.iterators.get_mut(key) {
                match &mut iterator.state {
                    IteratorState::Array(state)
                    | IteratorState::ArrayKeys(state)
                    | IteratorState::ArrayEntries(state) => state.next_index += 1,
                    IteratorState::String(state) => state.next_index += 1,
                    IteratorState::MapEntries(state)
                    | IteratorState::MapKeys(state)
                    | IteratorState::MapValues(state) => state.next_index += 1,
                    IteratorState::SetEntries(state) | IteratorState::SetValues(state) => {
                        state.next_index += 1
                    }
                }
            }
            self.refresh_iterator_accounting(key)?;
        }

        Ok(match value {
            Some(value) => (value, false),
            None => (Value::Undefined, true),
        })
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
                if let Some(value) = object.properties.get(&key) {
                    return Ok(value.clone());
                }
                if matches!(object.kind, ObjectKind::Console)
                    && let Some(value) = self.console_method(&key)
                {
                    return Ok(value);
                }
                Ok(Value::Undefined)
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
                } else if let Some(value) = array.properties.get(&key) {
                    Ok(value.clone())
                } else {
                    match key.as_str() {
                        "push" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayPush)),
                        "pop" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayPop)),
                        "slice" => Ok(Value::BuiltinFunction(BuiltinFunction::ArraySlice)),
                        "join" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayJoin)),
                        "includes" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayIncludes)),
                        "indexOf" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayIndexOf)),
                        "values" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayValues)),
                        "keys" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayKeys)),
                        "entries" => Ok(Value::BuiltinFunction(BuiltinFunction::ArrayEntries)),
                        _ => Ok(Value::Undefined),
                    }
                }
            }
            Value::Map(map) => {
                let map = self
                    .maps
                    .get(map)
                    .ok_or_else(|| JsliteError::runtime("map missing"))?;
                match key.as_str() {
                    "size" => Ok(Value::Number(map.entries.len() as f64)),
                    "get" => Ok(Value::BuiltinFunction(BuiltinFunction::MapGet)),
                    "set" => Ok(Value::BuiltinFunction(BuiltinFunction::MapSet)),
                    "has" => Ok(Value::BuiltinFunction(BuiltinFunction::MapHas)),
                    "delete" => Ok(Value::BuiltinFunction(BuiltinFunction::MapDelete)),
                    "clear" => Ok(Value::BuiltinFunction(BuiltinFunction::MapClear)),
                    "entries" => Ok(Value::BuiltinFunction(BuiltinFunction::MapEntries)),
                    "keys" => Ok(Value::BuiltinFunction(BuiltinFunction::MapKeys)),
                    "values" => Ok(Value::BuiltinFunction(BuiltinFunction::MapValues)),
                    _ => Ok(Value::Undefined),
                }
            }
            Value::Set(set) => {
                let set = self
                    .sets
                    .get(set)
                    .ok_or_else(|| JsliteError::runtime("set missing"))?;
                match key.as_str() {
                    "size" => Ok(Value::Number(set.entries.len() as f64)),
                    "add" => Ok(Value::BuiltinFunction(BuiltinFunction::SetAdd)),
                    "has" => Ok(Value::BuiltinFunction(BuiltinFunction::SetHas)),
                    "delete" => Ok(Value::BuiltinFunction(BuiltinFunction::SetDelete)),
                    "clear" => Ok(Value::BuiltinFunction(BuiltinFunction::SetClear)),
                    "entries" => Ok(Value::BuiltinFunction(BuiltinFunction::SetEntries)),
                    "keys" => Ok(Value::BuiltinFunction(BuiltinFunction::SetKeys)),
                    "values" => Ok(Value::BuiltinFunction(BuiltinFunction::SetValues)),
                    _ => Ok(Value::Undefined),
                }
            }
            Value::Iterator(_) if key == "next" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::IteratorNext))
            }
            Value::Iterator(_) => Ok(Value::Undefined),
            Value::Promise(_) => match key.as_str() {
                "then" => Ok(Value::BuiltinFunction(BuiltinFunction::PromiseThen)),
                "catch" => Ok(Value::BuiltinFunction(BuiltinFunction::PromiseCatch)),
                "finally" => Ok(Value::BuiltinFunction(BuiltinFunction::PromiseFinally)),
                _ => Ok(Value::Undefined),
            },
            Value::BuiltinFunction(BuiltinFunction::ArrayCtor) if key == "isArray" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::ArrayIsArray))
            }
            Value::BuiltinFunction(BuiltinFunction::ObjectCtor) => match key.as_str() {
                "keys" => Ok(Value::BuiltinFunction(BuiltinFunction::ObjectKeys)),
                "values" => Ok(Value::BuiltinFunction(BuiltinFunction::ObjectValues)),
                "entries" => Ok(Value::BuiltinFunction(BuiltinFunction::ObjectEntries)),
                "hasOwn" => Ok(Value::BuiltinFunction(BuiltinFunction::ObjectHasOwn)),
                _ => Ok(Value::Undefined),
            },
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor) if key == "resolve" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::PromiseResolve))
            }
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor) if key == "reject" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::PromiseReject))
            }
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor) if key == "all" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::PromiseAll))
            }
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor) if key == "race" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::PromiseRace))
            }
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor) if key == "any" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::PromiseAny))
            }
            Value::BuiltinFunction(BuiltinFunction::PromiseCtor) if key == "allSettled" => {
                Ok(Value::BuiltinFunction(BuiltinFunction::PromiseAllSettled))
            }
            Value::String(value) => match key.as_str() {
                "length" => Ok(Value::Number(value.chars().count() as f64)),
                "trim" => Ok(Value::BuiltinFunction(BuiltinFunction::StringTrim)),
                "includes" => Ok(Value::BuiltinFunction(BuiltinFunction::StringIncludes)),
                "startsWith" => Ok(Value::BuiltinFunction(BuiltinFunction::StringStartsWith)),
                "endsWith" => Ok(Value::BuiltinFunction(BuiltinFunction::StringEndsWith)),
                "slice" => Ok(Value::BuiltinFunction(BuiltinFunction::StringSlice)),
                "substring" => Ok(Value::BuiltinFunction(BuiltinFunction::StringSubstring)),
                "toLowerCase" => Ok(Value::BuiltinFunction(BuiltinFunction::StringToLowerCase)),
                "toUpperCase" => Ok(Value::BuiltinFunction(BuiltinFunction::StringToUpperCase)),
                _ => {
                    let _ = value;
                    Ok(Value::Undefined)
                }
            },
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
                self.objects
                    .get_mut(object)
                    .ok_or_else(|| JsliteError::runtime("object missing"))?
                    .properties
                    .insert(key, value);
                self.refresh_object_accounting(object)?;
                Ok(())
            }
            Value::Array(array) => {
                {
                    let array_ref = self
                        .arrays
                        .get_mut(array)
                        .ok_or_else(|| JsliteError::runtime("array missing"))?;
                    if let Ok(index) = key.parse::<usize>() {
                        if index >= array_ref.elements.len() {
                            array_ref.elements.resize(index + 1, Value::Undefined);
                        }
                        array_ref.elements[index] = value;
                    } else {
                        array_ref.properties.insert(key, value);
                    }
                }
                self.refresh_array_accounting(array)?;
                Ok(())
            }
            Value::Map(_) => Err(JsliteError::runtime(
                "TypeError: custom properties on Map values are not supported",
            )),
            Value::Set(_) => Err(JsliteError::runtime(
                "TypeError: custom properties on Set values are not supported",
            )),
            _ => Err(JsliteError::runtime("TypeError: value is not an object")),
        }
    }

    fn console_method(&self, key: &str) -> Option<Value> {
        let capability = match key {
            "log" => "console.log",
            "warn" => "console.warn",
            "error" => "console.error",
            _ => return None,
        };
        self.capability_value(capability)
    }

    fn capability_value(&self, name: &str) -> Option<Value> {
        let cell = self.find_cell(self.globals, name)?;
        let cell = self.cells.get(cell)?;
        if !cell.initialized {
            return None;
        }
        match &cell.value {
            Value::HostFunction(_) => Some(cell.value.clone()),
            _ => None,
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
                    Value::Object(_)
                    | Value::Array(_)
                    | Value::Map(_)
                    | Value::Set(_)
                    | Value::Iterator(_)
                    | Value::Promise(_) => "object",
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
            | Value::Map(_)
            | Value::Set(_)
            | Value::Iterator(_)
            | Value::Object(_)
            | Value::Promise(_)
            | Value::Closure(_)
            | Value::BuiltinFunction(_)
            | Value::HostFunction(_) => {
                return Err(JsliteError::runtime(
                    "cannot coerce complex value to number",
                ));
            }
        })
    }

    fn to_integer(&self, value: Value) -> JsliteResult<i64> {
        let number = self.to_number(value)?;
        if number.is_nan() || number == 0.0 {
            Ok(0)
        } else if number.is_infinite() {
            Ok(if number.is_sign_positive() {
                i64::MAX
            } else {
                i64::MIN
            })
        } else {
            let truncated = number.trunc();
            if truncated >= i64::MAX as f64 {
                Ok(i64::MAX)
            } else if truncated <= i64::MIN as f64 {
                Ok(i64::MIN)
            } else {
                Ok(truncated as i64)
            }
        }
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
            Value::Map(_) => "[object Map]".to_string(),
            Value::Set(_) => "[object Set]".to_string(),
            Value::Object(object) => self
                .error_summary(object)?
                .unwrap_or_else(|| "[object Object]".to_string()),
            Value::Iterator(_) => "[object Iterator]".to_string(),
            Value::Promise(_) => "[object Promise]".to_string(),
            Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_) => {
                "[Function]".to_string()
            }
        })
    }

    fn make_error_object(
        &mut self,
        name: &str,
        args: &[Value],
        code: Option<String>,
        details: Option<Value>,
    ) -> JsliteResult<Value> {
        let message = if let Some(value) = args.first() {
            self.to_string(value.clone())?
        } else {
            String::new()
        };
        let mut properties = IndexMap::from([
            ("name".to_string(), Value::String(name.to_string())),
            ("message".to_string(), Value::String(message)),
        ]);
        if let Some(code) = code {
            properties.insert("code".to_string(), Value::String(code));
        }
        if let Some(details) = details {
            properties.insert("details".to_string(), details);
        }
        let object = self.insert_object(properties, ObjectKind::Error(name.to_string()))?;
        Ok(Value::Object(object))
    }

    fn value_from_runtime_message(&mut self, message: &str) -> JsliteResult<Value> {
        let (name, detail) = match message.split_once(": ") {
            Some((name, detail)) if name == "Error" || name.ends_with("Error") => {
                (name.to_string(), detail.to_string())
            }
            _ => ("Error".to_string(), message.to_string()),
        };
        self.make_error_object(&name, &[Value::String(detail)], None, None)
    }

    fn value_from_host_error(&mut self, error: HostError) -> JsliteResult<Value> {
        let details = match error.details {
            Some(details) => Some(self.value_from_structured(details)?),
            None => None,
        };
        self.make_error_object(
            &error.name,
            &[Value::String(error.message)],
            error.code,
            details,
        )
    }

    fn render_exception(&self, value: &Value) -> JsliteResult<String> {
        match value {
            Value::Object(object) => {
                if let Some(summary) = self.error_summary(*object)? {
                    Ok(summary)
                } else {
                    self.to_string(value.clone())
                }
            }
            _ => self.to_string(value.clone()),
        }
    }

    fn error_summary(&self, object: ObjectKey) -> JsliteResult<Option<String>> {
        let object = self
            .objects
            .get(object)
            .ok_or_else(|| JsliteError::runtime("object missing"))?;
        let name = object.properties.get("name").and_then(|value| match value {
            Value::String(value) => Some(value.as_str()),
            _ => None,
        });
        let message = object
            .properties
            .get("message")
            .and_then(|value| match value {
                Value::String(value) => Some(value.as_str()),
                _ => None,
            });

        if !matches!(object.kind, ObjectKind::Error(_)) && name.is_none() && message.is_none() {
            return Ok(None);
        }

        let mut summary = match (name, message) {
            (Some(name), Some("")) => name.to_string(),
            (Some(name), Some(message)) => format!("{name}: {message}"),
            (Some(name), None) => name.to_string(),
            (None, Some(message)) => message.to_string(),
            (None, None) => "Error".to_string(),
        };

        if let Some(Value::String(code)) = object.properties.get("code") {
            summary.push_str(&format!(" [code={code}]"));
        }
        if let Some(details) = object.properties.get("details") {
            let details = self.value_to_structured(details.clone())?;
            summary.push_str(&format!(" [details={details:?}]"));
        }

        Ok(Some(summary))
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
            return Err(limit_error("instruction budget exhausted"));
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
                let array = self.insert_array(values, IndexMap::new())?;
                Value::Array(array)
            }
            StructuredValue::Object(object) => {
                let mut properties = IndexMap::new();
                for (key, value) in object {
                    properties.insert(key, self.value_from_structured(value)?);
                }
                let object = self.insert_object(properties, ObjectKind::Plain)?;
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
            Value::Map(_) | Value::Set(_) => {
                return Err(JsliteError::runtime(
                    "Map and Set values cannot cross the structured host boundary",
                ));
            }
            Value::Iterator(_)
            | Value::Promise(_)
            | Value::Closure(_)
            | Value::BuiltinFunction(_)
            | Value::HostFunction(_) => {
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
                let array = self.insert_array(values, IndexMap::new())?;
                Ok(Value::Array(array))
            }
            serde_json::Value::Object(object) => {
                let mut properties = IndexMap::new();
                for (key, value) in object {
                    properties.insert(key, self.value_from_json(value)?);
                }
                let object = self.insert_object(properties, ObjectKind::Plain)?;
                Ok(Value::Object(object))
            }
        }
    }
}

fn limit_error(message: impl Into<String>) -> JsliteError {
    JsliteError::Message {
        kind: DiagnosticKind::Limit,
        message: message.into(),
        span: None,
        traceback: Vec::new(),
    }
}

fn serialization_error(message: impl Into<String>) -> JsliteError {
    JsliteError::Message {
        kind: DiagnosticKind::Serialization,
        message: message.into(),
        span: None,
        traceback: Vec::new(),
    }
}

fn extra_value_bytes(value: &Value) -> usize {
    match value {
        Value::String(value) | Value::HostFunction(value) => value.len(),
        _ => 0,
    }
}

fn measure_bindings_bytes(bindings: &IndexMap<String, CellKey>) -> usize {
    bindings.len() * std::mem::size_of::<(String, CellKey)>()
        + bindings.keys().map(|key| key.len()).sum::<usize>()
}

fn measure_properties_bytes(properties: &IndexMap<String, Value>) -> usize {
    properties.len() * std::mem::size_of::<(String, Value)>()
        + properties
            .iter()
            .map(|(key, value)| key.len() + extra_value_bytes(value))
            .sum::<usize>()
}

fn measure_env_bytes(env: &Env) -> usize {
    std::mem::size_of::<Env>() + measure_bindings_bytes(&env.bindings)
}

fn measure_cell_bytes(cell: &Cell) -> usize {
    std::mem::size_of::<Cell>() + extra_value_bytes(&cell.value)
}

fn measure_object_bytes(object: &PlainObject) -> usize {
    std::mem::size_of::<PlainObject>()
        + measure_properties_bytes(&object.properties)
        + match &object.kind {
            ObjectKind::Error(name) => name.len(),
            _ => 0,
        }
}

fn measure_array_bytes(array: &ArrayObject) -> usize {
    std::mem::size_of::<ArrayObject>()
        + array.elements.len() * std::mem::size_of::<Value>()
        + array.elements.iter().map(extra_value_bytes).sum::<usize>()
        + measure_properties_bytes(&array.properties)
}

fn measure_map_bytes(map: &MapObject) -> usize {
    std::mem::size_of::<MapObject>()
        + map.entries.len() * std::mem::size_of::<MapEntry>()
        + map
            .entries
            .iter()
            .map(|entry| extra_value_bytes(&entry.key) + extra_value_bytes(&entry.value))
            .sum::<usize>()
}

fn measure_set_bytes(set: &SetObject) -> usize {
    std::mem::size_of::<SetObject>()
        + set.entries.len() * std::mem::size_of::<Value>()
        + set.entries.iter().map(extra_value_bytes).sum::<usize>()
}

fn measure_iterator_bytes(iterator: &IteratorObject) -> usize {
    let state_bytes = match &iterator.state {
        IteratorState::String(state) => state.value.len(),
        IteratorState::Array(_)
        | IteratorState::ArrayKeys(_)
        | IteratorState::ArrayEntries(_)
        | IteratorState::MapEntries(_)
        | IteratorState::MapKeys(_)
        | IteratorState::MapValues(_)
        | IteratorState::SetEntries(_)
        | IteratorState::SetValues(_) => 0,
    };
    std::mem::size_of::<IteratorObject>() + state_bytes
}

fn measure_closure_bytes(_closure: &Closure) -> usize {
    std::mem::size_of::<Closure>()
}

fn measure_promise_bytes(promise: &PromiseObject) -> usize {
    let state_bytes = match &promise.state {
        PromiseState::Pending => 0,
        PromiseState::Fulfilled(value) => extra_value_bytes(value),
        PromiseState::Rejected(rejection) => {
            extra_value_bytes(&rejection.value)
                + rejection
                    .traceback
                    .iter()
                    .map(|frame| frame.function_name.as_ref().map_or(0, String::len))
                    .sum::<usize>()
        }
    };
    let reaction_bytes = promise
        .reactions
        .iter()
        .map(|reaction| match reaction {
            PromiseReaction::Then {
                on_fulfilled,
                on_rejected,
                ..
            } => on_fulfilled
                .iter()
                .chain(on_rejected.iter())
                .map(extra_value_bytes)
                .sum::<usize>(),
            PromiseReaction::Finally { callback, .. } => {
                callback.iter().map(extra_value_bytes).sum::<usize>()
            }
            PromiseReaction::FinallyPassThrough {
                original_outcome, ..
            } => match original_outcome {
                PromiseOutcome::Fulfilled(value) => extra_value_bytes(value),
                PromiseOutcome::Rejected(rejection) => extra_value_bytes(&rejection.value),
            },
            PromiseReaction::Combinator { .. } => 0,
        })
        .sum::<usize>();
    let driver_bytes = match &promise.driver {
        Some(PromiseDriver::All { values, .. }) => values
            .iter()
            .flatten()
            .map(extra_value_bytes)
            .sum::<usize>(),
        Some(PromiseDriver::AllSettled { results, .. }) => results
            .iter()
            .flatten()
            .map(|result| match result {
                PromiseSettledResult::Fulfilled(value) | PromiseSettledResult::Rejected(value) => {
                    extra_value_bytes(value)
                }
            })
            .sum::<usize>(),
        Some(PromiseDriver::Any { reasons, .. }) => reasons
            .iter()
            .flatten()
            .map(extra_value_bytes)
            .sum::<usize>(),
        None => 0,
    };
    std::mem::size_of::<PromiseObject>()
        + promise.awaiters.len() * std::mem::size_of::<AsyncContinuation>()
        + promise.dependents.len() * std::mem::size_of::<PromiseKey>()
        + promise.reactions.len() * std::mem::size_of::<PromiseReaction>()
        + state_bytes
        + reaction_bytes
        + driver_bytes
}

fn pop_many(stack: &mut Vec<Value>, count: usize) -> JsliteResult<Vec<Value>> {
    if stack.len() < count {
        return Err(JsliteError::runtime("stack underflow"));
    }
    let start = stack.len() - count;
    Ok(stack.drain(start..).collect())
}

fn resume_behavior_for_capability(capability: &str) -> ResumeBehavior {
    match capability {
        "console.log" | "console.warn" | "console.error" => ResumeBehavior::Undefined,
        _ => ResumeBehavior::Value,
    }
}

fn instruction_may_allocate(instruction: &Instruction) -> bool {
    matches!(
        instruction,
        Instruction::StoreName(_)
            | Instruction::InitializePattern(_)
            | Instruction::PushEnv
            | Instruction::DeclareName { .. }
            | Instruction::MakeClosure { .. }
            | Instruction::MakeArray { .. }
            | Instruction::MakeObject { .. }
            | Instruction::CreateIterator
            | Instruction::SetPropStatic { .. }
            | Instruction::SetPropComputed
            | Instruction::Call { .. }
            | Instruction::Await
            | Instruction::Construct { .. }
    )
}

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Undefined | Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => *value != 0.0 && !value.is_nan(),
        Value::String(value) => !value.is_empty(),
        Value::Object(_)
        | Value::Array(_)
        | Value::Map(_)
        | Value::Set(_)
        | Value::Iterator(_)
        | Value::Promise(_)
        | Value::Closure(_)
        | Value::BuiltinFunction(_)
        | Value::HostFunction(_) => true,
    }
}

fn is_callable(value: &Value) -> bool {
    matches!(
        value,
        Value::Closure(_) | Value::BuiltinFunction(_) | Value::HostFunction(_)
    )
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
        (Value::Map(left), Value::Map(right)) => left == right,
        (Value::Set(left), Value::Set(right)) => left == right,
        (Value::Iterator(left), Value::Iterator(right)) => left == right,
        (Value::Promise(left), Value::Promise(right)) => left == right,
        (Value::Closure(left), Value::Closure(right)) => left == right,
        (Value::BuiltinFunction(left), Value::BuiltinFunction(right)) => left == right,
        _ => false,
    }
}

fn same_value_zero(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => {
            (left == right) || (left.is_nan() && right.is_nan())
        }
        _ => strict_equal(left, right),
    }
}

fn canonicalize_collection_key(value: Value) -> Value {
    match value {
        Value::Number(number) if number == 0.0 && number.is_sign_negative() => Value::Number(0.0),
        other => other,
    }
}

fn normalize_relative_bound(index: i64, len: usize) -> usize {
    let len = len as i64;
    if index < 0 {
        (len + index).max(0) as usize
    } else {
        index.min(len) as usize
    }
}

fn normalize_search_index(index: i64, len: usize) -> usize {
    if index < 0 {
        normalize_relative_bound(index, len)
    } else {
        clamp_index(index, len)
    }
}

fn clamp_index(index: i64, len: usize) -> usize {
    index.max(0).min(len as i64) as usize
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
            rest: None,
            code,
            is_async: false,
            is_arrow: false,
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
    fn preserves_supported_enumeration_order_for_json_stringify() {
        let value = run(r#"
            const record = {};
            record.beta = "b";
            record.alpha = "a";
            const values = ["c", "d"];
            values.extra = "ignored";
            JSON.stringify({ record, values });
            "#);
        assert_eq!(
            value,
            StructuredValue::String(
                r#"{"record":{"alpha":"a","beta":"b"},"values":["c","d"]}"#.to_string()
            )
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
                cancellation_token: None,
            },
        )
        .expect_err("infinite loop should exhaust budget");
        assert!(error.to_string().contains("instruction budget exhausted"));
    }

    #[test]
    fn tracks_heap_growth_and_enforces_heap_limits() {
        let program = lower_to_bytecode(&compile("1;").expect("source should compile"))
            .expect("lowering should succeed");
        let mut runtime = Runtime::new(program.clone(), ExecutionOptions::default())
            .expect("runtime should initialize");

        let baseline_heap = runtime.heap_bytes_used;
        let array = runtime
            .insert_array(vec![Value::String("payload".to_string())], IndexMap::new())
            .expect("array allocation should succeed");
        assert!(runtime.heap_bytes_used > baseline_heap);

        let array_heap = runtime.heap_bytes_used;
        runtime
            .set_property(
                Value::Array(array),
                Value::String("extra".to_string()),
                Value::String("more payload".to_string()),
            )
            .expect("array growth should succeed");
        assert!(runtime.heap_bytes_used > array_heap);

        let mut heap_limited = Runtime::new(program.clone(), ExecutionOptions::default())
            .expect("runtime should initialize");
        heap_limited.limits.heap_limit_bytes = heap_limited.heap_bytes_used;
        let error = heap_limited
            .insert_array(vec![Value::String("payload".to_string())], IndexMap::new())
            .expect_err("next allocation should exceed the heap limit");
        assert!(error.to_string().contains("heap limit exceeded"));

        let mut allocation_limited =
            Runtime::new(program, ExecutionOptions::default()).expect("runtime should initialize");
        allocation_limited.limits.allocation_budget = allocation_limited.allocation_count;
        let error = allocation_limited
            .insert_object(IndexMap::new(), ObjectKind::Plain)
            .expect_err("next allocation should exhaust the allocation budget");
        assert!(error.to_string().contains("allocation budget exhausted"));
    }

    #[test]
    fn iterators_participate_in_heap_accounting_and_gc() {
        let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
            .expect("lowering should succeed");
        let mut runtime = Runtime::new(program, ExecutionOptions::default()).expect("runtime init");

        let baseline_heap = runtime.heap_bytes_used;
        let kept_array = runtime
            .insert_array(
                vec![Value::Number(1.0), Value::Number(2.0)],
                IndexMap::new(),
            )
            .expect("kept array should allocate");
        let kept_iterator = runtime
            .insert_iterator(IteratorState::Array(ArrayIteratorState {
                array: kept_array,
                next_index: 1,
            }))
            .expect("kept iterator should allocate");
        assert!(runtime.heap_bytes_used > baseline_heap);

        let frame_env = runtime
            .new_env(Some(runtime.globals))
            .expect("frame env should allocate");
        let iterator_cell = runtime
            .insert_cell(Value::Iterator(kept_iterator), true, true)
            .expect("iterator cell should allocate");
        runtime
            .envs
            .get_mut(frame_env)
            .expect("frame env should exist")
            .bindings
            .insert("\0kept_iter".to_string(), iterator_cell);
        runtime
            .refresh_env_accounting(frame_env)
            .expect("frame env accounting should refresh");
        runtime.frames.push(Frame {
            function_id: 0,
            ip: 0,
            env: frame_env,
            scope_stack: Vec::new(),
            stack: Vec::new(),
            handlers: Vec::new(),
            pending_exception: None,
            pending_completions: Vec::new(),
            active_finally: Vec::new(),
            async_promise: None,
        });

        let garbage_array = runtime
            .insert_array(vec![Value::Number(9.0)], IndexMap::new())
            .expect("garbage array should allocate");
        let garbage_iterator = runtime
            .insert_iterator(IteratorState::Array(ArrayIteratorState {
                array: garbage_array,
                next_index: 0,
            }))
            .expect("garbage iterator should allocate");

        runtime.collect_garbage().expect("gc should succeed");
        assert!(runtime.arrays.contains_key(kept_array));
        assert!(runtime.iterators.contains_key(kept_iterator));
        assert!(!runtime.arrays.contains_key(garbage_array));
        assert!(!runtime.iterators.contains_key(garbage_iterator));

        runtime.frames.clear();
        runtime.collect_garbage().expect("gc should succeed");
        assert!(!runtime.arrays.contains_key(kept_array));
        assert!(!runtime.iterators.contains_key(kept_iterator));
    }

    #[test]
    fn map_and_set_iterators_keep_keyed_collections_alive_for_gc() {
        let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
            .expect("lowering should succeed");
        let mut runtime = Runtime::new(program, ExecutionOptions::default()).expect("runtime init");

        let kept_map = runtime.insert_map(Vec::new()).expect("map should allocate");
        runtime
            .map_set(
                kept_map,
                Value::String("alpha".to_string()),
                Value::Number(1.0),
            )
            .expect("map entry should insert");
        let kept_set = runtime.insert_set(Vec::new()).expect("set should allocate");
        runtime
            .set_add(kept_set, Value::String("beta".to_string()))
            .expect("set entry should insert");
        let kept_map_iterator = runtime
            .insert_iterator(IteratorState::MapEntries(MapIteratorState {
                map: kept_map,
                next_index: 0,
            }))
            .expect("map iterator should allocate");
        let kept_set_iterator = runtime
            .insert_iterator(IteratorState::SetValues(SetIteratorState {
                set: kept_set,
                next_index: 0,
            }))
            .expect("set iterator should allocate");

        let frame_env = runtime
            .new_env(Some(runtime.globals))
            .expect("frame env should allocate");
        let map_iterator_cell = runtime
            .insert_cell(Value::Iterator(kept_map_iterator), true, true)
            .expect("map iterator cell should allocate");
        let set_iterator_cell = runtime
            .insert_cell(Value::Iterator(kept_set_iterator), true, true)
            .expect("set iterator cell should allocate");
        let env = runtime
            .envs
            .get_mut(frame_env)
            .expect("frame env should exist");
        env.bindings
            .insert("\0kept_map_iter".to_string(), map_iterator_cell);
        env.bindings
            .insert("\0kept_set_iter".to_string(), set_iterator_cell);
        runtime
            .refresh_env_accounting(frame_env)
            .expect("frame env accounting should refresh");
        runtime.frames.push(Frame {
            function_id: 0,
            ip: 0,
            env: frame_env,
            scope_stack: Vec::new(),
            stack: Vec::new(),
            handlers: Vec::new(),
            pending_exception: None,
            pending_completions: Vec::new(),
            active_finally: Vec::new(),
            async_promise: None,
        });

        let garbage_map = runtime.insert_map(Vec::new()).expect("map should allocate");
        runtime
            .map_set(
                garbage_map,
                Value::String("gamma".to_string()),
                Value::Number(2.0),
            )
            .expect("garbage map entry should insert");
        let garbage_set = runtime.insert_set(Vec::new()).expect("set should allocate");
        runtime
            .set_add(garbage_set, Value::String("delta".to_string()))
            .expect("garbage set entry should insert");
        let garbage_map_iterator = runtime
            .insert_iterator(IteratorState::MapEntries(MapIteratorState {
                map: garbage_map,
                next_index: 0,
            }))
            .expect("garbage map iterator should allocate");
        let garbage_set_iterator = runtime
            .insert_iterator(IteratorState::SetValues(SetIteratorState {
                set: garbage_set,
                next_index: 0,
            }))
            .expect("garbage set iterator should allocate");

        runtime.collect_garbage().expect("gc should succeed");
        assert!(runtime.maps.contains_key(kept_map));
        assert!(runtime.sets.contains_key(kept_set));
        assert!(runtime.iterators.contains_key(kept_map_iterator));
        assert!(runtime.iterators.contains_key(kept_set_iterator));
        assert!(!runtime.maps.contains_key(garbage_map));
        assert!(!runtime.sets.contains_key(garbage_set));
        assert!(!runtime.iterators.contains_key(garbage_map_iterator));
        assert!(!runtime.iterators.contains_key(garbage_set_iterator));
    }

    #[test]
    fn promise_reactions_keep_target_promises_alive_for_gc() {
        let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
            .expect("lowering should succeed");
        let mut runtime = Runtime::new(program, ExecutionOptions::default()).expect("runtime init");

        let kept_source = runtime
            .insert_promise(PromiseState::Pending)
            .expect("source promise should allocate");
        let kept_target = runtime
            .insert_promise(PromiseState::Pending)
            .expect("target promise should allocate");
        runtime
            .attach_promise_reaction(
                kept_source,
                PromiseReaction::Then {
                    target: kept_target,
                    on_fulfilled: None,
                    on_rejected: None,
                },
            )
            .expect("reaction should attach");

        let frame_env = runtime
            .new_env(Some(runtime.globals))
            .expect("frame env should allocate");
        let source_cell = runtime
            .insert_cell(Value::Promise(kept_source), true, true)
            .expect("promise cell should allocate");
        runtime
            .envs
            .get_mut(frame_env)
            .expect("frame env should exist")
            .bindings
            .insert("\0kept_promise".to_string(), source_cell);
        runtime
            .refresh_env_accounting(frame_env)
            .expect("frame env accounting should refresh");
        runtime.frames.push(Frame {
            function_id: 0,
            ip: 0,
            env: frame_env,
            scope_stack: Vec::new(),
            stack: Vec::new(),
            handlers: Vec::new(),
            pending_exception: None,
            pending_completions: Vec::new(),
            active_finally: Vec::new(),
            async_promise: None,
        });

        let garbage_source = runtime
            .insert_promise(PromiseState::Pending)
            .expect("garbage promise should allocate");
        let garbage_target = runtime
            .insert_promise(PromiseState::Pending)
            .expect("garbage target should allocate");
        runtime
            .attach_promise_reaction(
                garbage_source,
                PromiseReaction::Then {
                    target: garbage_target,
                    on_fulfilled: None,
                    on_rejected: None,
                },
            )
            .expect("garbage reaction should attach");

        runtime.collect_garbage().expect("gc should succeed");
        assert!(runtime.promises.contains_key(kept_source));
        assert!(runtime.promises.contains_key(kept_target));
        assert!(!runtime.promises.contains_key(garbage_source));
        assert!(!runtime.promises.contains_key(garbage_target));
    }

    #[test]
    fn maps_preserve_insertion_order_and_same_value_zero_updates() {
        let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
            .expect("lowering should succeed");
        let mut runtime = Runtime::new(program, ExecutionOptions::default()).expect("runtime init");
        let map = runtime.insert_map(Vec::new()).expect("map should allocate");
        let object = runtime
            .insert_object(IndexMap::new(), ObjectKind::Plain)
            .expect("object should allocate");

        runtime
            .map_set(map, Value::String("alpha".to_string()), Value::Number(1.0))
            .expect("alpha insert should succeed");
        runtime
            .map_set(
                map,
                Value::Number(f64::NAN),
                Value::String("nan".to_string()),
            )
            .expect("nan insert should succeed");
        runtime
            .map_set(map, Value::Number(-0.0), Value::String("zero".to_string()))
            .expect("negative zero insert should succeed");
        runtime
            .map_set(map, Value::Object(object), Value::Bool(true))
            .expect("object key insert should succeed");
        runtime
            .map_set(map, Value::String("alpha".to_string()), Value::Number(2.0))
            .expect("alpha update should keep insertion order");
        runtime
            .map_set(
                map,
                Value::Number(0.0),
                Value::String("zero-updated".to_string()),
            )
            .expect("positive zero update should reuse the existing entry");

        let entries = &runtime.maps.get(map).expect("map should exist").entries;
        assert_eq!(entries.len(), 4);
        assert!(matches!(entries[0].key, Value::String(ref value) if value == "alpha"));
        assert!(matches!(entries[0].value, Value::Number(value) if value == 2.0));
        assert!(matches!(entries[1].key, Value::Number(value) if value.is_nan()));
        assert!(matches!(entries[1].value, Value::String(ref value) if value == "nan"));
        assert!(matches!(entries[2].key, Value::Number(value) if value == 0.0));
        assert!(matches!(entries[2].value, Value::String(ref value) if value == "zero-updated"));
        assert!(matches!(entries[3].key, Value::Object(key) if key == object));
        assert!(matches!(entries[3].value, Value::Bool(true)));
    }

    #[test]
    fn keyed_collections_participate_in_heap_accounting_and_gc() {
        let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
            .expect("lowering should succeed");
        let mut runtime = Runtime::new(program, ExecutionOptions::default()).expect("runtime init");

        let baseline_heap = runtime.heap_bytes_used;
        let kept_map = runtime.insert_map(Vec::new()).expect("map should allocate");
        let kept_set = runtime.insert_set(Vec::new()).expect("set should allocate");
        runtime
            .map_set(
                kept_map,
                Value::String("set".to_string()),
                Value::Set(kept_set),
            )
            .expect("map should store the set");
        runtime
            .set_add(kept_set, Value::Map(kept_map))
            .expect("set should store the map");
        assert!(runtime.heap_bytes_used > baseline_heap);

        let frame_env = runtime
            .new_env(Some(runtime.globals))
            .expect("frame env should allocate");
        let map_cell = runtime
            .insert_cell(Value::Map(kept_map), true, true)
            .expect("map cell should allocate");
        runtime
            .envs
            .get_mut(frame_env)
            .expect("frame env should exist")
            .bindings
            .insert("\0kept_map".to_string(), map_cell);
        runtime
            .refresh_env_accounting(frame_env)
            .expect("frame env accounting should refresh");
        runtime.frames.push(Frame {
            function_id: 0,
            ip: 0,
            env: frame_env,
            scope_stack: Vec::new(),
            stack: Vec::new(),
            handlers: Vec::new(),
            pending_exception: None,
            pending_completions: Vec::new(),
            active_finally: Vec::new(),
            async_promise: None,
        });

        let garbage_map = runtime
            .insert_map(Vec::new())
            .expect("garbage map should allocate");
        let garbage_set = runtime
            .insert_set(Vec::new())
            .expect("garbage set should allocate");
        runtime
            .map_set(
                garbage_map,
                Value::String("set".to_string()),
                Value::Set(garbage_set),
            )
            .expect("garbage map should store the set");
        runtime
            .set_add(garbage_set, Value::Map(garbage_map))
            .expect("garbage set should store the map");

        runtime.collect_garbage().expect("gc should succeed");
        assert!(runtime.maps.contains_key(kept_map));
        assert!(runtime.sets.contains_key(kept_set));
        assert!(!runtime.maps.contains_key(garbage_map));
        assert!(!runtime.sets.contains_key(garbage_set));

        runtime.frames.clear();
        runtime.collect_garbage().expect("gc should succeed");
        assert!(!runtime.maps.contains_key(kept_map));
        assert!(!runtime.sets.contains_key(kept_set));
    }

    #[test]
    fn garbage_collection_marks_runtime_roots_and_collects_cycles() {
        let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
            .expect("lowering should succeed");
        let mut runtime =
            Runtime::new(program.clone(), ExecutionOptions::default()).expect("runtime init");

        let closure_env = runtime
            .new_env(Some(runtime.globals))
            .expect("closure env should allocate");
        let rooted_closure = runtime
            .insert_closure(program.root, closure_env)
            .expect("closure should allocate");
        let rooted_object = runtime
            .insert_object(
                IndexMap::from([("closure".to_string(), Value::Closure(rooted_closure))]),
                ObjectKind::Plain,
            )
            .expect("object should allocate");
        let rooted_array = runtime
            .insert_array(vec![Value::Object(rooted_object)], IndexMap::new())
            .expect("array should allocate");

        let frame_env = runtime
            .new_env(Some(runtime.globals))
            .expect("frame env should allocate");
        let rooted_cell = runtime
            .insert_cell(Value::Array(rooted_array), true, true)
            .expect("cell should allocate");
        runtime
            .envs
            .get_mut(frame_env)
            .expect("frame env should exist")
            .bindings
            .insert("kept".to_string(), rooted_cell);
        runtime
            .refresh_env_accounting(frame_env)
            .expect("frame env accounting should refresh");
        runtime.frames.push(Frame {
            function_id: program.root,
            ip: 0,
            env: frame_env,
            scope_stack: vec![closure_env],
            stack: vec![Value::Closure(rooted_closure)],
            handlers: vec![ExceptionHandler {
                catch: None,
                finally: None,
                env: closure_env,
                scope_stack_len: 0,
                stack_len: 0,
            }],
            pending_exception: Some(Value::Object(rooted_object)),
            pending_completions: vec![
                CompletionRecord::Return(Value::Array(rooted_array)),
                CompletionRecord::Throw(Value::Closure(rooted_closure)),
            ],
            active_finally: Vec::new(),
            async_promise: None,
        });

        let garbage_env = runtime.new_env(None).expect("garbage env should allocate");
        let garbage_left = runtime
            .insert_object(IndexMap::new(), ObjectKind::Plain)
            .expect("garbage object should allocate");
        let garbage_right = runtime
            .insert_object(IndexMap::new(), ObjectKind::Plain)
            .expect("garbage object should allocate");
        let garbage_array = runtime
            .insert_array(vec![Value::Object(garbage_left)], IndexMap::new())
            .expect("garbage array should allocate");
        let garbage_closure = runtime
            .insert_closure(program.root, garbage_env)
            .expect("garbage closure should allocate");
        runtime
            .set_property(
                Value::Object(garbage_left),
                Value::String("peer".to_string()),
                Value::Object(garbage_right),
            )
            .expect("left cycle should update");
        runtime
            .set_property(
                Value::Object(garbage_right),
                Value::String("peer".to_string()),
                Value::Object(garbage_left),
            )
            .expect("right cycle should update");
        runtime
            .set_property(
                Value::Object(garbage_right),
                Value::String("items".to_string()),
                Value::Array(garbage_array),
            )
            .expect("array cycle should update");
        runtime
            .set_property(
                Value::Object(garbage_left),
                Value::String("closure".to_string()),
                Value::Closure(garbage_closure),
            )
            .expect("closure cycle should update");
        let garbage_cell = runtime
            .insert_cell(Value::Object(garbage_left), true, true)
            .expect("garbage cell should allocate");
        runtime
            .envs
            .get_mut(garbage_env)
            .expect("garbage env should exist")
            .bindings
            .insert("garbage".to_string(), garbage_cell);
        runtime
            .refresh_env_accounting(garbage_env)
            .expect("garbage env accounting should refresh");

        let stats = runtime.collect_garbage().expect("gc should succeed");

        assert!(stats.reclaimed_allocations >= 5);
        assert!(stats.reclaimed_bytes > 0);
        assert!(runtime.envs.contains_key(frame_env));
        assert!(runtime.envs.contains_key(closure_env));
        assert!(runtime.cells.contains_key(rooted_cell));
        assert!(runtime.objects.contains_key(rooted_object));
        assert!(runtime.arrays.contains_key(rooted_array));
        assert!(runtime.closures.contains_key(rooted_closure));

        assert!(!runtime.envs.contains_key(garbage_env));
        assert!(!runtime.cells.contains_key(garbage_cell));
        assert!(!runtime.objects.contains_key(garbage_left));
        assert!(!runtime.objects.contains_key(garbage_right));
        assert!(!runtime.arrays.contains_key(garbage_array));
        assert!(!runtime.closures.contains_key(garbage_closure));
    }

    #[test]
    fn garbage_collection_reclaims_cyclic_garbage_under_execution_pressure() {
        let program = compile(
            r#"
            let total = 0;
            for (let i = 0; i < 120; i += 1) {
              let left = {};
              let right = {};
              left.peer = right;
              right.peer = left;
              total += i;
            }
            total;
            "#,
        )
        .expect("source should compile");
        let value = execute(
            &program,
            ExecutionOptions {
                limits: RuntimeLimits {
                    heap_limit_bytes: 24 * 1024,
                    allocation_budget: 256,
                    ..RuntimeLimits::default()
                },
                cancellation_token: None,
                ..ExecutionOptions::default()
            },
        )
        .expect("cyclic garbage should be reclaimed");
        assert_eq!(
            value,
            StructuredValue::Number(StructuredNumber::Finite(7140.0))
        );
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
    fn console_callbacks_resume_with_undefined_guest_results() {
        let program = compile(
            r#"
            const logged = console.log(41);
            logged === undefined ? 2 : 0;
            "#,
        )
        .expect("source should compile");

        let step = start(
            &program,
            ExecutionOptions {
                capabilities: vec!["console.log".to_string()],
                ..ExecutionOptions::default()
            },
        )
        .expect("execution should suspend on console.log");

        let suspension = match step {
            ExecutionStep::Suspended(suspension) => suspension,
            other => panic!("expected suspension, got {other:?}"),
        };
        assert_eq!(suspension.capability, "console.log");
        assert_eq!(
            suspension.args,
            vec![StructuredValue::Number(StructuredNumber::Finite(41.0))]
        );

        let resumed = resume(
            suspension.snapshot,
            ResumePayload::Value(StructuredValue::String("ignored".to_string())),
        )
        .expect("resume should ignore host return values for console callbacks");

        match resumed {
            ExecutionStep::Completed(value) => {
                assert_eq!(
                    value,
                    StructuredValue::Number(StructuredNumber::Finite(2.0))
                );
            }
            other => panic!("expected completion, got {other:?}"),
        }
    }

    #[test]
    fn runs_throw_try_catch_and_finally() {
        let value = run(r#"
            let log = [];
            try {
              log[log.length] = "body";
              throw new Error("boom");
            } catch (error) {
              log[log.length] = error.name;
              log[log.length] = error.message;
            } finally {
              log[log.length] = "finally";
            }
            log;
            "#);
        assert_eq!(
            value,
            StructuredValue::Array(vec![
                StructuredValue::String("body".to_string()),
                StructuredValue::String("Error".to_string()),
                StructuredValue::String("boom".to_string()),
                StructuredValue::String("finally".to_string()),
            ])
        );
    }

    #[test]
    fn catches_runtime_type_errors_as_guest_errors() {
        let value = run(r#"
            let captured;
            try {
              const value = null;
              value.answer;
            } catch (error) {
              captured = [error.name, error.message];
            }
            captured;
            "#);
        assert_eq!(
            value,
            StructuredValue::Array(vec![
                StructuredValue::String("TypeError".to_string()),
                StructuredValue::String("cannot read properties of nullish value".to_string()),
            ])
        );
    }

    #[test]
    fn finally_runs_for_return_break_and_continue() {
        let value = run(r#"
            let events = [];
            function earlyReturn() {
              try {
                return "body";
              } finally {
                events[events.length] = "return";
              }
            }
            let index = 0;
            while (index < 2) {
              index += 1;
              try {
                if (index === 1) {
                  continue;
                }
                break;
              } finally {
                events[events.length] = index;
              }
            }
            [earlyReturn(), events];
            "#);
        assert_eq!(
            value,
            StructuredValue::Array(vec![
                StructuredValue::String("body".to_string()),
                StructuredValue::Array(vec![
                    StructuredValue::Number(StructuredNumber::Finite(1.0)),
                    StructuredValue::Number(StructuredNumber::Finite(2.0)),
                    StructuredValue::String("return".to_string()),
                ]),
            ])
        );
    }

    #[test]
    fn nested_exception_unwind_preserves_finally_order() {
        let value = run(r#"
            let events = [];
            function nested() {
              try {
                try {
                  events[events.length] = "inner-body";
                  throw new Error("boom");
                } catch (error) {
                  events[events.length] = error.message;
                  throw new TypeError("wrapped");
                } finally {
                  events[events.length] = "inner-finally";
                }
              } catch (error) {
                events[events.length] = error.name;
              } finally {
                events[events.length] = "outer-finally";
              }
              return events;
            }
            nested();
            "#);
        assert_eq!(
            value,
            StructuredValue::Array(vec![
                StructuredValue::String("inner-body".to_string()),
                StructuredValue::String("boom".to_string()),
                StructuredValue::String("inner-finally".to_string()),
                StructuredValue::String("TypeError".to_string()),
                StructuredValue::String("outer-finally".to_string()),
            ])
        );
    }

    #[test]
    fn catches_host_errors_after_resume() {
        let program = compile(
            r#"
            let captured;
            try {
              fetch_data(1);
            } catch (error) {
              captured = [error.name, error.message, error.code, error.details.status];
            }
            captured;
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
        .expect("execution should suspend");

        let suspension = match step {
            ExecutionStep::Suspended(suspension) => suspension,
            other => panic!("expected suspension, got {other:?}"),
        };

        let resumed = resume(
            suspension.snapshot,
            ResumePayload::Error(HostError {
                name: "CapabilityError".to_string(),
                message: "upstream failed".to_string(),
                code: Some("E_UPSTREAM".to_string()),
                details: Some(StructuredValue::Object(IndexMap::from([(
                    "status".to_string(),
                    StructuredValue::Number(StructuredNumber::Finite(503.0)),
                )]))),
            }),
        )
        .expect("guest catch should handle resumed host errors");

        match resumed {
            ExecutionStep::Completed(value) => {
                assert_eq!(
                    value,
                    StructuredValue::Array(vec![
                        StructuredValue::String("CapabilityError".to_string()),
                        StructuredValue::String("upstream failed".to_string()),
                        StructuredValue::String("E_UPSTREAM".to_string()),
                        StructuredValue::Number(StructuredNumber::Finite(503.0)),
                    ])
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
                .contains("has inconsistent validation state")
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
