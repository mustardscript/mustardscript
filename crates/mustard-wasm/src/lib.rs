use indexmap::IndexMap;
use mustard::structured::StructuredNumber;
use mustard::{
    Diagnostic, DiagnosticKind, ExecutionOptions, ExecutionSnapshot, ExecutionStep, HostError,
    ResumePayload, RuntimeDebugMetrics, RuntimeLimits, StructuredValue, compile, lower_to_bytecode,
    resume_with_options_and_metrics, start_shared_bytecode_with_metrics,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::slice;
use std::str;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};

fn snapshots() -> &'static Mutex<HashMap<u64, ExecutionSnapshot>> {
    static SNAPSHOTS: OnceLock<Mutex<HashMap<u64, ExecutionSnapshot>>> = OnceLock::new();
    SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_snapshot_handle() -> u64 {
    static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
    NEXT_HANDLE.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Deserialize)]
struct StartRequest {
    code: String,
    #[serde(default)]
    inputs: IndexMap<String, JsonValue>,
    #[serde(default)]
    capabilities: Vec<String>,
    limits: Option<RuntimeLimits>,
}

#[derive(Debug, Deserialize)]
struct ResumeRequest {
    handle: u64,
    payload: ResumePayloadDto,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ResumePayloadDto {
    Value { value: JsonValue },
    Error { error: HostErrorDto },
    Cancelled,
}

#[derive(Debug, Deserialize)]
struct HostErrorDto {
    #[serde(default = "default_error_name")]
    name: String,
    #[serde(default)]
    message: String,
    code: Option<String>,
    details: Option<JsonValue>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum WasmResponse {
    Completed {
        value: JsonValue,
        metrics: RuntimeDebugMetrics,
    },
    Suspended {
        handle: u64,
        capability: String,
        args: Vec<JsonValue>,
        metrics: RuntimeDebugMetrics,
    },
    Error {
        error: WasmError,
    },
}

#[derive(Debug, Serialize)]
struct WasmError {
    name: String,
    message: String,
    span: Option<SpanDto>,
    diagnostics: Option<Vec<DiagnosticDto>>,
}

#[derive(Debug, Serialize)]
struct SpanDto {
    start: u32,
    end: u32,
}

#[derive(Debug, Serialize)]
struct DiagnosticDto {
    kind: String,
    message: String,
    span: Option<SpanDto>,
}

fn default_error_name() -> String {
    "Error".to_string()
}

fn handle_start_request(request: StartRequest) -> WasmResponse {
    let inputs = match request
        .inputs
        .into_iter()
        .map(|(key, value)| plain_json_to_structured(value).map(|value| (key, value)))
        .collect::<Result<IndexMap<_, _>, _>>()
    {
        Ok(inputs) => inputs,
        Err(error) => return WasmResponse::Error { error },
    };

    let program = match compile(&request.code).and_then(|program| lower_to_bytecode(&program)) {
        Ok(program) => Arc::new(program),
        Err(error) => {
            return WasmResponse::Error {
                error: normalize_error(error),
            };
        }
    };

    let options = ExecutionOptions {
        inputs,
        capabilities: request.capabilities,
        limits: request.limits.unwrap_or_default(),
        cancellation_token: None,
    };

    match start_shared_bytecode_with_metrics(program, options) {
        Ok((step, metrics)) => materialize_step(step, metrics),
        Err(error) => WasmResponse::Error {
            error: normalize_error(error),
        },
    }
}

fn handle_resume_request(request: ResumeRequest) -> WasmResponse {
    let snapshot = match take_snapshot(request.handle) {
        Ok(snapshot) => snapshot,
        Err(error) => return WasmResponse::Error { error },
    };

    match resume_with_options_and_metrics(snapshot, request.payload.into(), Default::default()) {
        Ok((step, metrics)) => materialize_step(step, metrics),
        Err(error) => WasmResponse::Error {
            error: normalize_error(error),
        },
    }
}

fn materialize_step(step: ExecutionStep, metrics: RuntimeDebugMetrics) -> WasmResponse {
    match step {
        ExecutionStep::Completed(value) => WasmResponse::Completed {
            value: structured_to_json(value),
            metrics,
        },
        ExecutionStep::Suspended(suspension) => {
            let handle = next_snapshot_handle();
            if let Err(error) = store_snapshot(handle, suspension.snapshot) {
                return WasmResponse::Error { error };
            }
            WasmResponse::Suspended {
                handle,
                capability: suspension.capability,
                args: suspension
                    .args
                    .into_iter()
                    .map(structured_to_json)
                    .collect(),
                metrics,
            }
        }
    }
}

fn store_snapshot(handle: u64, snapshot: ExecutionSnapshot) -> Result<(), WasmError> {
    snapshots()
        .lock()
        .map_err(|_| host_error("snapshot registry is poisoned"))?
        .insert(handle, snapshot);
    Ok(())
}

fn take_snapshot(handle: u64) -> Result<ExecutionSnapshot, WasmError> {
    snapshots()
        .lock()
        .map_err(|_| host_error("snapshot registry is poisoned"))?
        .remove(&handle)
        .ok_or_else(|| host_error(format!("unknown snapshot handle `{handle}`")))
}

impl From<ResumePayloadDto> for ResumePayload {
    fn from(value: ResumePayloadDto) -> Self {
        match value {
            ResumePayloadDto::Value { value } => plain_json_to_structured(value)
                .map(Self::Value)
                .unwrap_or_else(|error| {
                    Self::Error(HostError {
                        name: error.name,
                        message: error.message,
                        code: None,
                        details: None,
                    })
                }),
            ResumePayloadDto::Error { error } => Self::Error(error.into()),
            ResumePayloadDto::Cancelled => Self::Cancelled,
        }
    }
}

impl From<HostErrorDto> for HostError {
    fn from(value: HostErrorDto) -> Self {
        Self {
            name: value.name,
            message: value.message,
            code: value.code,
            details: value
                .details
                .and_then(|value| plain_json_to_structured(value).ok()),
        }
    }
}

fn plain_json_to_structured(value: JsonValue) -> Result<StructuredValue, WasmError> {
    match value {
        JsonValue::Null => Ok(StructuredValue::Null),
        JsonValue::Bool(value) => Ok(StructuredValue::Bool(value)),
        JsonValue::String(value) => Ok(StructuredValue::String(value)),
        JsonValue::Number(value) => value.as_f64().map(StructuredValue::from).ok_or_else(|| {
            host_error("numbers that do not fit into f64 cannot cross the browser boundary")
        }),
        JsonValue::Array(values) => values
            .into_iter()
            .map(plain_json_to_structured)
            .collect::<Result<Vec<_>, _>>()
            .map(StructuredValue::Array),
        JsonValue::Object(mut entries) => {
            if let Some(meta) = entries.remove("$mustard")
                && let Some(tag) = meta.as_str()
            {
                return match tag {
                    "undefined" => Ok(StructuredValue::Undefined),
                    "hole" => Ok(StructuredValue::Hole),
                    "nan" => Ok(StructuredValue::Number(StructuredNumber::NaN)),
                    "infinity" => Ok(StructuredValue::Number(StructuredNumber::Infinity)),
                    "neg_infinity" => Ok(StructuredValue::Number(StructuredNumber::NegInfinity)),
                    "neg_zero" => Ok(StructuredValue::Number(StructuredNumber::NegZero)),
                    _ => Err(host_error(format!("unknown mustard sentinel `{tag}`"))),
                };
            }

            entries
                .into_iter()
                .map(|(key, value)| plain_json_to_structured(value).map(|value| (key, value)))
                .collect::<Result<IndexMap<_, _>, _>>()
                .map(StructuredValue::Object)
        }
    }
}

fn structured_to_json(value: StructuredValue) -> JsonValue {
    match value {
        StructuredValue::Undefined => sentinel("undefined"),
        StructuredValue::Null => JsonValue::Null,
        StructuredValue::Hole => sentinel("hole"),
        StructuredValue::Bool(value) => JsonValue::Bool(value),
        StructuredValue::String(value) => JsonValue::String(value),
        StructuredValue::Number(value) => match value {
            StructuredNumber::Finite(value) => serde_json::Number::from_f64(value)
                .map(JsonValue::Number)
                .unwrap_or_else(|| sentinel("nan")),
            StructuredNumber::NaN => sentinel("nan"),
            StructuredNumber::Infinity => sentinel("infinity"),
            StructuredNumber::NegInfinity => sentinel("neg_infinity"),
            StructuredNumber::NegZero => sentinel("neg_zero"),
        },
        StructuredValue::Array(values) => {
            JsonValue::Array(values.into_iter().map(structured_to_json).collect())
        }
        StructuredValue::Object(entries) => JsonValue::Object(serde_json::Map::from_iter(
            entries
                .into_iter()
                .map(|(key, value)| (key, structured_to_json(value))),
        )),
    }
}

fn sentinel(tag: &str) -> JsonValue {
    JsonValue::Object(serde_json::Map::from_iter([(
        "$mustard".to_string(),
        JsonValue::String(tag.to_string()),
    )]))
}

fn normalize_error(error: mustard::MustardError) -> WasmError {
    match error {
        mustard::MustardError::Diagnostics(diagnostics) => {
            let message = diagnostics
                .first()
                .map(|item| item.message.clone())
                .unwrap_or_else(|| "unknown diagnostics error".to_string());
            let span = diagnostics
                .first()
                .and_then(|item| item.span)
                .map(span_to_dto);
            let name = diagnostics
                .first()
                .map(|item| diagnostic_kind_name(&item.kind))
                .unwrap_or("MustardError")
                .to_string();
            WasmError {
                name,
                message,
                span,
                diagnostics: Some(diagnostics.iter().map(diagnostic_to_dto).collect()),
            }
        }
        mustard::MustardError::Message {
            kind,
            message,
            span,
            ..
        } => WasmError {
            name: diagnostic_kind_name(&kind).to_string(),
            message,
            span: span.map(span_to_dto),
            diagnostics: None,
        },
    }
}

fn host_error(message: impl Into<String>) -> WasmError {
    WasmError {
        name: "MustardWasmHostError".to_string(),
        message: message.into(),
        span: None,
        diagnostics: None,
    }
}

fn diagnostic_to_dto(diagnostic: &Diagnostic) -> DiagnosticDto {
    DiagnosticDto {
        kind: diagnostic_kind_name(&diagnostic.kind).to_string(),
        message: diagnostic.message.clone(),
        span: diagnostic.span.map(span_to_dto),
    }
}

fn diagnostic_kind_name(kind: &DiagnosticKind) -> &'static str {
    match kind {
        DiagnosticKind::Parse => "MustardParseError",
        DiagnosticKind::Validation => "MustardValidationError",
        DiagnosticKind::Runtime => "MustardRuntimeError",
        DiagnosticKind::Limit => "MustardLimitError",
        DiagnosticKind::Serialization => "MustardSerializationError",
    }
}

fn span_to_dto(span: mustard::span::SourceSpan) -> SpanDto {
    SpanDto {
        start: span.start,
        end: span.end,
    }
}

fn decode_json_slice<'a, T: Deserialize<'a>>(ptr: u32, len: u32) -> Result<T, WasmError> {
    if len == 0 {
        return Err(host_error("empty request body"));
    }
    let bytes = unsafe { slice::from_raw_parts(ptr as *const u8, len as usize) };
    let source = str::from_utf8(bytes)
        .map_err(|error| host_error(format!("request body is not valid UTF-8: {error}")))?;
    serde_json::from_str(source)
        .map_err(|error| host_error(format!("request body is not valid JSON: {error}")))
}

fn encode_response(response: &WasmResponse) -> u32 {
    let payload = serde_json::to_vec(response).unwrap_or_else(|error| {
        format!(
            "{{\"status\":\"error\",\"error\":{{\"name\":\"MustardWasmHostError\",\"message\":{message}}}}}",
            message = serde_json::to_string(&format!("failed to encode response: {error}")).unwrap()
        )
        .into_bytes()
    });
    pack_buffer(payload)
}

fn pack_buffer(payload: Vec<u8>) -> u32 {
    let len = payload.len();
    let layout = std::alloc::Layout::from_size_align(len + 4, 1).expect("valid response layout");
    let ptr = unsafe { std::alloc::alloc(layout) };
    if ptr.is_null() {
        return 0;
    }
    unsafe {
        (ptr as *mut u32).write_unaligned(len as u32);
        ptr.add(4).copy_from_nonoverlapping(payload.as_ptr(), len);
    }
    ptr as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn mustard_wasm_alloc(len: u32) -> u32 {
    let layout = match std::alloc::Layout::from_size_align(len as usize, 1) {
        Ok(layout) => layout,
        Err(_) => return 0,
    };
    let ptr = unsafe { std::alloc::alloc(layout) };
    ptr as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn mustard_wasm_free(ptr: u32, len: u32) {
    if ptr == 0 || len == 0 {
        return;
    }
    if let Ok(layout) = std::alloc::Layout::from_size_align(len as usize, 1) {
        unsafe { std::alloc::dealloc(ptr as *mut u8, layout) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn mustard_wasm_buffer_free(ptr: u32) {
    if ptr == 0 {
        return;
    }
    let len = unsafe { (ptr as *const u32).read_unaligned() as usize };
    if let Ok(layout) = std::alloc::Layout::from_size_align(len + 4, 1) {
        unsafe { std::alloc::dealloc(ptr as *mut u8, layout) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn mustard_wasm_start_json(ptr: u32, len: u32) -> u32 {
    let response = match decode_json_slice::<StartRequest>(ptr, len) {
        Ok(request) => handle_start_request(request),
        Err(error) => WasmResponse::Error { error },
    };
    encode_response(&response)
}

#[unsafe(no_mangle)]
pub extern "C" fn mustard_wasm_resume_json(ptr: u32, len: u32) -> u32 {
    let response = match decode_json_slice::<ResumeRequest>(ptr, len) {
        Ok(request) => handle_resume_request(request),
        Err(error) => WasmResponse::Error { error },
    };
    encode_response(&response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_compute_program_completes() {
        let response = handle_start_request(StartRequest {
            code: "const x = 2 + 2; x;".to_string(),
            inputs: IndexMap::new(),
            capabilities: Vec::new(),
            limits: Some(RuntimeLimits::default()),
        });

        match response {
            WasmResponse::Completed { value, .. } => assert_eq!(value, JsonValue::from(4.0)),
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn capability_suspension_round_trips() {
        let first = handle_start_request(StartRequest {
            code: "lookup_plan_policy(plan);".to_string(),
            inputs: IndexMap::from([("plan".to_string(), JsonValue::String("pro".to_string()))]),
            capabilities: vec!["lookup_plan_policy".to_string()],
            limits: Some(RuntimeLimits::default()),
        });

        let handle = match first {
            WasmResponse::Suspended {
                handle,
                capability,
                args,
                ..
            } => {
                assert_eq!(capability, "lookup_plan_policy");
                assert_eq!(args, vec![JsonValue::String("pro".to_string())]);
                handle
            }
            other => panic!("unexpected response: {other:?}"),
        };

        let resumed = handle_resume_request(ResumeRequest {
            handle,
            payload: ResumePayloadDto::Value {
                value: JsonValue::String("enterprise".to_string()),
            },
        });

        match resumed {
            WasmResponse::Completed {
                value: JsonValue::String(value),
                ..
            } => assert_eq!(value, "enterprise"),
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn unknown_sentinel_fails_closed() {
        let error = plain_json_to_structured(JsonValue::Object(serde_json::Map::from_iter([(
            "$mustard".to_string(),
            JsonValue::String("mystery".to_string()),
        )])))
        .expect_err("unknown sentinel should fail");

        assert_eq!(error.name, "MustardWasmHostError");
        assert!(error.message.contains("unknown mustard sentinel"));
    }

    #[test]
    fn unknown_snapshot_handle_fails_closed() {
        let response = handle_resume_request(ResumeRequest {
            handle: u64::MAX,
            payload: ResumePayloadDto::Cancelled,
        });

        match response {
            WasmResponse::Error { error } => {
                assert_eq!(error.name, "MustardWasmHostError");
                assert!(error.message.contains("unknown snapshot handle"));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn missing_capability_name_fails_closed() {
        let response = handle_start_request(StartRequest {
            code: "lookup_plan_policy(plan);".to_string(),
            inputs: IndexMap::from([("plan".to_string(), JsonValue::String("starter".to_string()))]),
            capabilities: Vec::new(),
            limits: Some(RuntimeLimits::default()),
        });

        match response {
            WasmResponse::Error { error } => {
                assert_eq!(error.name, "MustardRuntimeError");
                assert!(error.message.contains("lookup_plan_policy"));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
