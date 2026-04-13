use super::*;
use crate::runtime::compiler::pattern_bindings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ValidationState {
    stack_depth: usize,
    scope_depth: usize,
    handler_depth: usize,
    pending_depth: usize,
}

pub(in crate::runtime) fn validate_bytecode_program(
    program: &BytecodeProgram,
) -> MustardResult<()> {
    if program.functions.is_empty() {
        return Err(MustardError::validation(
            "bytecode validation failed: program defines no functions",
            None,
        ));
    }
    if program.root >= program.functions.len() {
        return Err(MustardError::validation(
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
) -> MustardResult<()> {
    if function.param_binding_names.len() != function.params.len() {
        return Err(MustardError::validation(
            format!(
                "bytecode validation failed: function {function_id} parameter binding metadata length {} does not match parameter count {}",
                function.param_binding_names.len(),
                function.params.len()
            ),
            None,
        ));
    }
    for (index, pattern) in function.params.iter().enumerate() {
        let expected: Vec<String> = pattern_bindings(pattern)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        if function.param_binding_names[index] != expected {
            return Err(MustardError::validation(
                format!(
                    "bytecode validation failed: function {function_id} parameter {index} binding metadata does not match the destructuring pattern"
                ),
                None,
            ));
        }
    }
    let expected_rest: Vec<String> = function
        .rest
        .iter()
        .flat_map(|pattern| pattern_bindings(pattern).into_iter().map(|(name, _)| name))
        .collect();
    if function.rest_binding_names != expected_rest {
        return Err(MustardError::validation(
            format!(
                "bytecode validation failed: function {function_id} rest binding metadata does not match the destructuring pattern"
            ),
            None,
        ));
    }
    if function.code.is_empty() {
        return Err(MustardError::validation(
            format!("bytecode validation failed: function {function_id} has no instructions"),
            None,
        ));
    }
    if !matches!(function.code.last(), Some(Instruction::Return)) {
        return Err(MustardError::validation(
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
                return Err(MustardError::validation(
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
                return Err(MustardError::validation(
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
                return Err(MustardError::validation(
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
                return Err(MustardError::validation(
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
) -> MustardResult<ValidationState> {
    let require_stack = |count: usize| -> MustardResult<()> {
        if state.stack_depth < count {
            return Err(MustardError::validation(
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
        | Instruction::PushBigInt(_)
        | Instruction::PushString(_)
        | Instruction::PushRegExp { .. }
        | Instruction::LoadSlot { .. }
        | Instruction::LoadName(_)
        | Instruction::LoadGlobal(_)
        | Instruction::LoadGlobalObject
        | Instruction::MakeClosure { .. }
        | Instruction::BeginCatch => ValidationState {
            stack_depth: state.stack_depth + 1,
            ..state
        },
        Instruction::StoreSlot { .. } | Instruction::StoreName(_) | Instruction::StoreGlobal(_) => {
            require_stack(1)?;
            state
        }
        Instruction::StoreSlotDiscard { .. }
        | Instruction::StoreNameDiscard(_)
        | Instruction::StoreGlobalDiscard(_) => {
            require_stack(1)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
                ..state
            }
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
                return Err(MustardError::validation(
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
        Instruction::ArrayPush => {
            require_stack(2)?;
            state
        }
        Instruction::ArrayPushHole => {
            require_stack(1)?;
            state
        }
        Instruction::ArrayExtend => {
            require_stack(2)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
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
        Instruction::CopyDataProperties => {
            require_stack(2)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
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
        Instruction::PatternArrayIndex(_)
        | Instruction::PatternArrayRest(_)
        | Instruction::PatternObjectRest(_)
        | Instruction::Update(_) => {
            require_stack(1)?;
            state
        }
        Instruction::SetPropStatic { .. } => {
            require_stack(2)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
                ..state
            }
        }
        Instruction::SetPropStaticDiscard { .. } => {
            require_stack(2)?;
            ValidationState {
                stack_depth: state.stack_depth - 2,
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
        Instruction::SetPropComputedDiscard => {
            require_stack(3)?;
            ValidationState {
                stack_depth: state.stack_depth - 3,
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
                return Err(MustardError::validation(
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
                return Err(MustardError::validation(
                    format!(
                        "bytecode validation failed: function {function_id} instruction {ip} targets handler depth {target_handler_depth} from depth {}",
                        state.handler_depth
                    ),
                    None,
                ));
            }
            if *target_scope_depth > state.scope_depth {
                return Err(MustardError::validation(
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
                return Err(MustardError::validation(
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
        Instruction::CallWithArray { with_this, .. } => {
            let required = 2 + usize::from(*with_this);
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
        Instruction::ConstructWithArray => {
            require_stack(2)?;
            ValidationState {
                stack_depth: state.stack_depth - 1,
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
