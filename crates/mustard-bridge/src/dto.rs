use std::collections::BTreeMap;

use anyhow::{Result, anyhow};
use mustard::{HostError, ResumePayload, RuntimeLimits, SnapshotPolicy, StructuredValue};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct StartOptionsDto {
    #[serde(default)]
    pub inputs: BTreeMap<String, StructuredValue>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub limits: RuntimeLimitsDto,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RuntimeLimitsDto {
    pub instruction_budget: Option<usize>,
    pub heap_limit_bytes: Option<usize>,
    pub allocation_budget: Option<usize>,
    pub call_depth_limit: Option<usize>,
    pub max_outstanding_host_calls: Option<usize>,
}

impl RuntimeLimitsDto {
    pub fn into_runtime_limits(self) -> RuntimeLimits {
        let defaults = RuntimeLimits::default();
        RuntimeLimits {
            instruction_budget: self
                .instruction_budget
                .unwrap_or(defaults.instruction_budget),
            heap_limit_bytes: self.heap_limit_bytes.unwrap_or(defaults.heap_limit_bytes),
            allocation_budget: self.allocation_budget.unwrap_or(defaults.allocation_budget),
            call_depth_limit: self.call_depth_limit.unwrap_or(defaults.call_depth_limit),
            max_outstanding_host_calls: self
                .max_outstanding_host_calls
                .unwrap_or(defaults.max_outstanding_host_calls),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotPolicyDto {
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub limits: Option<RuntimeLimitsDto>,
    #[serde(default)]
    pub snapshot_key_base64: Option<String>,
    #[serde(default)]
    pub snapshot_token: Option<String>,
    #[serde(default)]
    pub snapshot_id: Option<String>,
    #[serde(default)]
    pub snapshot_key_digest: Option<String>,
}

impl SnapshotPolicyDto {
    pub fn into_snapshot_policy(self) -> Result<SnapshotPolicy> {
        let limits = self
            .limits
            .ok_or_else(|| anyhow!("raw snapshot restore requires explicit limits"))?;
        Ok(SnapshotPolicy {
            capabilities: self.capabilities,
            limits: limits.into_runtime_limits(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepDto {
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
pub enum ResumeDto {
    Value { value: StructuredValue },
    Error { error: HostError },
    Cancelled,
}

impl ResumeDto {
    pub fn into_resume_payload(self) -> ResumePayload {
        match self {
            Self::Value { value } => ResumePayload::Value(value),
            Self::Error { error } => ResumePayload::Error(error),
            Self::Cancelled => ResumePayload::Cancelled,
        }
    }
}
