mod accounting;
mod api;
mod async_runtime;
mod builtins;
mod bytecode;
mod compiler;
mod conversions;
mod env;
mod exceptions;
mod gc;
#[cfg(test)]
mod gc_trigger_tests;
mod properties;
mod serialization;
mod shared;
#[cfg(test)]
mod snapshot_validation_tests;
mod state;
mod validation;
mod vm;

pub use api::{
    CollectionCallSiteMetrics, ExecutionOptions, ExecutionSnapshot, ExecutionStep, HostError,
    ResumeOptions, ResumePayload, RuntimeDebugMetrics, SnapshotInspection, SnapshotPolicy,
    Suspension, apply_snapshot_policy, execute, inspect_snapshot, resume, resume_with_options,
    resume_with_options_and_metrics, snapshot_inspection, start, start_bytecode,
    start_shared_bytecode, start_shared_bytecode_with_metrics, start_validated_bytecode,
};
pub use bytecode::{BytecodeProgram, FunctionPrototype, Instruction};
pub use compiler::lower_to_bytecode;
pub use serialization::{
    canonical_snapshot_auth_bytes, dump_detached_snapshot, dump_program, dump_snapshot,
    load_detached_snapshot, load_program, load_snapshot,
};

use indexmap::IndexMap;
use slotmap::SlotMap;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, OnceLock};

use self::properties::{
    array_index_from_property_key, format_number_key, ordered_own_property_keys,
    property_name_to_key,
};
use self::shared::{
    CallbackCallOptions, is_callable, is_truthy, limit_error, next_snapshot_nonce, pop_many,
    resume_behavior_for_capability, same_value_zero, serialization_error, strict_equal,
};
use self::state::*;
use crate::{
    cancellation::CancellationToken,
    diagnostic::{DiagnosticKind, MustardError, MustardResult, TraceFrame},
    ir::{BinaryOp, Pattern, PropertyName, UnaryOp},
    limits::RuntimeLimits,
    span::SourceSpan,
    structured::{StructuredNumber, StructuredValue},
};

const INTERNAL_CALLBACK_THROW_MARKER: &str = "\0internal-array-callback-throw";

fn runtime_image() -> &'static RuntimeImage {
    static RUNTIME_IMAGE: OnceLock<RuntimeImage> = OnceLock::new();
    RUNTIME_IMAGE.get_or_init(|| {
        Runtime::build_runtime_image().expect("builtin runtime image should initialize")
    })
}

impl Runtime {
    fn new(program: Arc<BytecodeProgram>, options: ExecutionOptions) -> MustardResult<Self> {
        let ExecutionOptions {
            inputs,
            capabilities,
            limits,
            cancellation_token,
        } = options;
        let image = runtime_image();
        if image.heap_bytes_used > limits.heap_limit_bytes {
            return Err(limit_error("heap limit exceeded"));
        }
        if image.allocation_count > limits.allocation_budget {
            return Err(limit_error("allocation budget exhausted"));
        }
        let mut runtime = Self::from_runtime_image(image, program, limits, cancellation_token);
        for capability in capabilities {
            runtime.define_global(capability.clone(), Value::HostFunction(capability), false)?;
        }
        for (name, value) in inputs {
            let value = runtime.value_from_structured(value)?;
            runtime.define_global(name, value, true)?;
        }
        runtime.reset_gc_debt();
        Ok(runtime)
    }

    fn blank(
        program: Arc<BytecodeProgram>,
        limits: RuntimeLimits,
        cancellation_token: Option<CancellationToken>,
    ) -> Self {
        let mut envs = SlotMap::with_key();
        let globals = envs.insert(Env {
            parent: None,
            bindings: IndexMap::new(),
            accounted_bytes: 0,
        });
        Self {
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
            builtin_prototypes: IndexMap::new(),
            builtin_function_objects: IndexMap::new(),
            host_function_objects: IndexMap::new(),
            object_shapes: HashMap::new(),
            next_object_shape_id: 1,
            static_property_inline_caches: HashMap::new(),
            property_feedback_sites: HashMap::new(),
            builtin_feedback_sites: HashMap::new(),
            collection_call_sites: HashMap::new(),
            snapshot_nonce: next_snapshot_nonce(),
            instruction_counter: 0,
            heap_bytes_used: 0,
            allocation_count: 0,
            gc_allocation_debt_bytes: 0,
            gc_allocation_debt_count: 0,
            debug_metrics: RuntimeDebugMetrics::default(),
            operation_counters_enabled: false,
            accounting_recount_required: false,
            cancellation_token,
            regex_cache: HashMap::new(),
            pending_internal_exception: None,
            pending_sync_callback_result: None,
            snapshot_policy_required: false,
            pending_resume_behavior: ResumeBehavior::Value,
        }
    }

    fn from_runtime_image(
        image: &RuntimeImage,
        program: Arc<BytecodeProgram>,
        limits: RuntimeLimits,
        cancellation_token: Option<CancellationToken>,
    ) -> Self {
        Self {
            program,
            limits,
            globals: image.globals,
            envs: image.envs.clone(),
            cells: image.cells.clone(),
            objects: image.objects.clone(),
            arrays: image.arrays.clone(),
            maps: image.maps.clone(),
            sets: image.sets.clone(),
            iterators: image.iterators.clone(),
            closures: image.closures.clone(),
            promises: image.promises.clone(),
            frames: Vec::new(),
            root_result: None,
            microtasks: VecDeque::new(),
            pending_host_calls: VecDeque::new(),
            suspended_host_call: None,
            builtin_prototypes: image.builtin_prototypes.clone(),
            builtin_function_objects: image.builtin_function_objects.clone(),
            host_function_objects: image.host_function_objects.clone(),
            object_shapes: HashMap::new(),
            next_object_shape_id: 1,
            static_property_inline_caches: HashMap::new(),
            property_feedback_sites: HashMap::new(),
            builtin_feedback_sites: HashMap::new(),
            collection_call_sites: HashMap::new(),
            snapshot_nonce: next_snapshot_nonce(),
            instruction_counter: 0,
            heap_bytes_used: image.heap_bytes_used,
            allocation_count: image.allocation_count,
            gc_allocation_debt_bytes: 0,
            gc_allocation_debt_count: 0,
            debug_metrics: RuntimeDebugMetrics::default(),
            operation_counters_enabled: false,
            accounting_recount_required: false,
            cancellation_token,
            regex_cache: HashMap::new(),
            pending_internal_exception: None,
            pending_sync_callback_result: None,
            snapshot_policy_required: false,
            pending_resume_behavior: ResumeBehavior::Value,
        }
    }

    fn build_runtime_image() -> MustardResult<RuntimeImage> {
        let bootstrap_program = Arc::new(BytecodeProgram {
            functions: vec![FunctionPrototype {
                name: None,
                length: 0,
                display_source: String::new(),
                params: Vec::new(),
                param_binding_names: Vec::new(),
                rest: None,
                rest_binding_names: Vec::new(),
                code: vec![Instruction::PushUndefined, Instruction::Return],
                is_async: false,
                is_arrow: false,
                span: SourceSpan::new(0, 0),
            }],
            root: 0,
        });
        let mut runtime = Self::blank(bootstrap_program, RuntimeLimits::default(), None);
        runtime.account_existing_env(runtime.globals)?;
        runtime.install_builtins()?;
        Ok(RuntimeImage {
            globals: runtime.globals,
            envs: runtime.envs,
            cells: runtime.cells,
            objects: runtime.objects,
            arrays: runtime.arrays,
            maps: runtime.maps,
            sets: runtime.sets,
            iterators: runtime.iterators,
            closures: runtime.closures,
            promises: runtime.promises,
            builtin_prototypes: runtime.builtin_prototypes,
            builtin_function_objects: runtime.builtin_function_objects,
            host_function_objects: runtime.host_function_objects,
            heap_bytes_used: runtime.heap_bytes_used,
            allocation_count: runtime.allocation_count,
        })
    }

    fn apply_resume_options(&mut self, options: ResumeOptions) -> MustardResult<()> {
        if options.cancellation_token.is_some() {
            self.cancellation_token = options.cancellation_token;
        }
        if let Some(policy) = options.snapshot_policy {
            self.apply_snapshot_policy(policy)?;
        }
        if self.snapshot_policy_required {
            return Err(serialization_error(
                "loaded snapshots require explicit host policy before resume",
            ));
        }
        Ok(())
    }

    fn apply_snapshot_policy(&mut self, policy: SnapshotPolicy) -> MustardResult<()> {
        validation::validate_snapshot_policy(self, &policy)?;
        self.limits = policy.limits;
        if self.frames.len() > self.limits.call_depth_limit {
            return Err(limit_error("call depth limit exceeded"));
        }
        self.enforce_loaded_accounting()?;
        let outstanding_host_calls =
            self.pending_host_calls.len() + usize::from(self.suspended_host_call.is_some());
        if outstanding_host_calls > self.limits.max_outstanding_host_calls {
            return Err(limit_error("outstanding host-call limit exhausted"));
        }
        if self.instruction_counter > self.limits.instruction_budget {
            return Err(limit_error("instruction budget exhausted"));
        }
        self.snapshot_policy_required = false;
        Ok(())
    }

    fn check_cancellation(&self) -> MustardResult<()> {
        if self
            .cancellation_token
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            return Err(limit_error("execution cancelled"));
        }
        Ok(())
    }

    pub(crate) fn with_temporary_roots<T, F>(&mut self, roots: &[Value], f: F) -> MustardResult<T>
    where
        F: FnOnce(&mut Self) -> MustardResult<T>,
    {
        let frame_index = self.frames.len().checked_sub(1).ok_or_else(|| {
            MustardError::runtime("no active frame available for temporary roots")
        })?;
        let original_len = self.frames[frame_index].stack.len();
        self.frames[frame_index].stack.extend(roots.iter().cloned());
        let result = f(self);
        if let Some(frame) = self.frames.get_mut(frame_index) {
            frame.stack.truncate(original_len);
        }
        result
    }

    pub(crate) fn call_callback(
        &mut self,
        callback: Value,
        this_arg: Value,
        args: &[Value],
        options: CallbackCallOptions<'_>,
    ) -> MustardResult<Value> {
        match callback {
            Value::Closure(closure) => {
                let base_depth = self.frames.len();
                match self.call_callable(Value::Closure(closure), this_arg, args)? {
                    RunState::Completed(value) => Ok(value),
                    RunState::PushedFrame => {
                        self.finish_pushed_callback_frame(base_depth, &options)
                    }
                    RunState::StartedAsync(value) => Ok(value),
                    RunState::Suspended { .. } => {
                        Err(MustardError::runtime(options.host_suspension_message))
                    }
                }
            }
            Value::BuiltinFunction(function)
                if matches!(
                    function,
                    BuiltinFunction::FunctionCall
                        | BuiltinFunction::FunctionApply
                        | BuiltinFunction::FunctionBind
                ) =>
            {
                let base_depth = self.frames.len();
                match self.call_callable(Value::BuiltinFunction(function), this_arg, args)? {
                    RunState::Completed(value) => Ok(value),
                    RunState::PushedFrame => {
                        self.finish_pushed_callback_frame(base_depth, &options)
                    }
                    RunState::StartedAsync(value) => Ok(value),
                    RunState::Suspended { .. } => {
                        Err(MustardError::runtime(options.host_suspension_message))
                    }
                }
            }
            Value::BuiltinFunction(function) => self.call_builtin(function, this_arg, args),
            Value::Object(object)
                if self
                    .objects
                    .get(object)
                    .is_some_and(|object| matches!(object.kind, ObjectKind::BoundFunction(_))) =>
            {
                let base_depth = self.frames.len();
                match self.call_callable(Value::Object(object), this_arg, args)? {
                    RunState::Completed(value) => Ok(value),
                    RunState::PushedFrame => {
                        self.finish_pushed_callback_frame(base_depth, &options)
                    }
                    RunState::StartedAsync(value) => Ok(value),
                    RunState::Suspended { .. } => {
                        Err(MustardError::runtime(options.host_suspension_message))
                    }
                }
            }
            Value::HostFunction(capability) => {
                if !options.allow_host_suspension || self.current_async_boundary_index().is_none() {
                    return Err(MustardError::runtime(options.host_suspension_message));
                }
                let outstanding =
                    self.pending_host_calls.len() + usize::from(self.suspended_host_call.is_some());
                if outstanding >= self.limits.max_outstanding_host_calls {
                    return Err(limit_error("outstanding host-call limit exhausted"));
                }
                let args = args
                    .iter()
                    .cloned()
                    .map(|value| self.value_to_structured(value))
                    .collect::<MustardResult<Vec<_>>>()?;
                let promise = self.insert_promise(PromiseState::Pending)?;
                self.pending_host_calls.push_back(PendingHostCall {
                    capability,
                    args,
                    promise: Some(promise),
                    resume_behavior: ResumeBehavior::Value,
                    traceback: self.traceback_snapshots(),
                });
                Ok(Value::Promise(promise))
            }
            _ => Err(MustardError::runtime(options.non_callable_message)),
        }
    }

    fn finish_pushed_callback_frame(
        &mut self,
        base_depth: usize,
        options: &CallbackCallOptions<'_>,
    ) -> MustardResult<Value> {
        self.pending_sync_callback_result = None;
        let frame = self
            .frames
            .last_mut()
            .ok_or_else(|| MustardError::runtime("missing callback frame"))?;
        frame.callback_capture = true;

        if let Err(error) = self.run_until_frame_depth(base_depth, options.host_suspension_message)
        {
            if self.frames.len() > base_depth
                && let Some(frame) = self.frames.last_mut()
            {
                frame.callback_capture = false;
            }
            self.pending_sync_callback_result = None;
            return Err(error);
        }

        if self.pending_internal_exception.is_some() {
            return Err(MustardError::runtime(INTERNAL_CALLBACK_THROW_MARKER));
        }

        let value = self
            .pending_sync_callback_result
            .take()
            .ok_or_else(|| MustardError::runtime("missing callback result"))?;

        if !options.allow_pending_promise_result
            && let Value::Promise(promise) = value
        {
            return match self.promise_outcome(promise)? {
                Some(PromiseOutcome::Fulfilled(value)) => Ok(value),
                Some(PromiseOutcome::Rejected(rejection)) => {
                    self.pending_internal_exception = Some(rejection);
                    Err(MustardError::runtime(INTERNAL_CALLBACK_THROW_MARKER))
                }
                None => Err(MustardError::runtime(options.unsettled_message)),
            };
        }

        Ok(value)
    }

    fn bump_instruction_budget(&mut self) -> MustardResult<()> {
        self.instruction_counter += 1;
        if self.instruction_counter > self.limits.instruction_budget {
            return Err(limit_error("instruction budget exhausted"));
        }
        Ok(())
    }

    pub(super) fn charge_native_helper_work(&mut self, units: usize) -> MustardResult<()> {
        if units == 0 {
            return Ok(());
        }
        self.check_cancellation()?;
        self.instruction_counter = self
            .instruction_counter
            .checked_add(units)
            .ok_or_else(|| limit_error("instruction budget exhausted"))?;
        if self.instruction_counter > self.limits.instruction_budget {
            return Err(limit_error("instruction budget exhausted"));
        }
        Ok(())
    }
}
