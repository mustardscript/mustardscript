use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RuntimeLimits {
    pub instruction_budget: usize,
    pub heap_limit_bytes: usize,
    pub allocation_budget: usize,
    pub call_depth_limit: usize,
    pub max_outstanding_host_calls: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            instruction_budget: 1_000_000,
            heap_limit_bytes: 8 * 1024 * 1024,
            allocation_budget: 250_000,
            call_depth_limit: 256,
            max_outstanding_host_calls: 128,
        }
    }
}
