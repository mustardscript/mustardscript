use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use jslite::{
    BytecodeProgram, ExecutionOptions, ExecutionStep, HostError, ResumePayload, StructuredValue,
    compile, dump_program, dump_snapshot, load_program, load_snapshot, lower_to_bytecode, resume,
    start_bytecode,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct StartOptionsDto {
    #[serde(default)]
    inputs: std::collections::BTreeMap<String, StructuredValue>,
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StepDto {
    Completed {
        value: StructuredValue,
    },
    Suspended {
        capability: String,
        args: Vec<StructuredValue>,
        snapshot_base64: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ResumeDto {
    Value { value: StructuredValue },
    Error { error: HostError },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
enum Request {
    Compile {
        id: u64,
        source: String,
    },
    Start {
        id: u64,
        program_base64: String,
        options: StartOptionsDto,
    },
    Resume {
        id: u64,
        snapshot_base64: String,
        payload: ResumeDto,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    id: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ResponsePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ResponsePayload {
    Program { program_base64: String },
    Step { step: StepDto },
}

fn encode_step(step: ExecutionStep) -> Result<StepDto> {
    Ok(match step {
        ExecutionStep::Completed(value) => StepDto::Completed { value },
        ExecutionStep::Suspended(suspension) => StepDto::Suspended {
            capability: suspension.capability,
            args: suspension.args,
            snapshot_base64: STANDARD.encode(dump_snapshot(&suspension.snapshot)?),
        },
    })
}

fn decode_program(base64: &str) -> Result<BytecodeProgram> {
    let bytes = STANDARD.decode(base64)?;
    Ok(load_program(&bytes)?)
}

fn handle(request: Request) -> Response {
    let id = match &request {
        Request::Compile { id, .. } | Request::Start { id, .. } | Request::Resume { id, .. } => *id,
    };

    let result: Result<ResponsePayload> = match request {
        Request::Compile { source, .. } => (|| {
            let program = compile(&source)?;
            let bytecode = lower_to_bytecode(&program)?;
            let bytes = dump_program(&bytecode)?;
            Ok(ResponsePayload::Program {
                program_base64: STANDARD.encode(bytes),
            })
        })(),
        Request::Start {
            program_base64,
            options,
            ..
        } => (|| {
            let program = decode_program(&program_base64)?;
            let step = start_bytecode(
                &program,
                ExecutionOptions {
                    inputs: options.inputs.into_iter().collect(),
                    capabilities: options.capabilities,
                    ..ExecutionOptions::default()
                },
            )?;
            Ok(ResponsePayload::Step {
                step: encode_step(step)?,
            })
        })(),
        Request::Resume {
            snapshot_base64,
            payload,
            ..
        } => (|| {
            let snapshot_bytes = STANDARD.decode(snapshot_base64)?;
            let snapshot = load_snapshot(&snapshot_bytes)?;
            let payload = match payload {
                ResumeDto::Value { value } => ResumePayload::Value(value),
                ResumeDto::Error { error } => ResumePayload::Error(error),
            };
            let step = resume(snapshot, payload)?;
            Ok(ResponsePayload::Step {
                step: encode_step(step)?,
            })
        })(),
    };

    match result {
        Ok(result) => Response {
            id,
            ok: true,
            result: Some(result),
            error: None,
        },
        Err(error) => Response {
            id,
            ok: false,
            result: None,
            error: Some(error.to_string()),
        },
    }
}

fn main() -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.context("failed to read request line")?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Request = serde_json::from_str(&line).context("invalid request")?;
        let response = handle(request);
        serde_json::to_writer(&mut stdout, &response).context("failed to write response")?;
        writeln!(&mut stdout).context("failed to terminate response line")?;
        stdout.flush().context("failed to flush response")?;
    }
    Ok(())
}
