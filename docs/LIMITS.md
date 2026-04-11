# Limits

`jslite` exposes a limits struct early so resource controls stay part of the
public contract even while enforcement is still being filled in.

## Public Limits

The runtime exposes:

- instruction budget
- heap byte budget
- allocation count budget
- call-depth budget
- maximum outstanding host calls
- cancellation control

The Node wrapper exposes these fields through `run()` and `start()` as:

- `limits.instructionBudget`
- `limits.heapLimitBytes`
- `limits.allocationBudget`
- `limits.callDepthLimit`
- `limits.maxOutstandingHostCalls`

## Defaults

`RuntimeLimits::default()` currently sets:

- `instruction_budget = 1_000_000`
- `heap_limit_bytes = 8 * 1024 * 1024`
- `allocation_budget = 250_000`
- `call_depth_limit = 256`
- `max_outstanding_host_calls = 128`

## Current Enforcement Status

- Instruction budgeting is implemented and enforced on every executed
  instruction.
- Heap byte limits, allocation-count limits, call-depth limits, and outstanding
  host-call limits are defined in the API but are not enforced yet.
- Explicit cancellation is not implemented yet.

## Default Policy

- Limits are enabled by default.
- Cancellation is planned as a cooperative mechanism checked at defined
  execution points.
- Over-budget execution fails with guest-safe runtime errors.
