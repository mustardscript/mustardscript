use std::collections::{HashSet, VecDeque};

use crate::diagnostic::{DiagnosticKind, JsliteError, JsliteResult};

use super::*;

mod bytecode;
mod policy;
mod snapshot;
mod walk;

pub(super) use bytecode::validate_bytecode_program;
pub(super) use policy::validate_snapshot_policy;
pub(super) use snapshot::validate_snapshot;
