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

Cooperative cancellation is controlled separately through:

- `options.signal` on `run()` and `start()`
- `Progress.resume(..., { signal })` / `Progress.resumeError(..., { signal })`
- `Progress.cancel()` for a currently suspended execution

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
- Heap byte limits and allocation-count limits are implemented and enforced with
  conservative accounting across live guest heap allocations and heap-backed
  mutations.
- The runtime runs a non-moving mark-sweep collection pass at allocation-safe
  execution boundaries and on resume before failing heap or allocation
  pressure, so unreachable cycles can be reclaimed without changing handle
  identities.
- Snapshot load recomputes heap accounting before resuming so serialized inputs
  cannot bypass the configured heap and allocation budgets.
- Outstanding host-call limits are enforced for async guest execution across
  queued and currently suspended host capability requests.
- Call-depth limits are still defined in the API but are not enforced yet.
- Cooperative cancellation is implemented and checked before each instruction,
  before idle microtask or queued-host-call checkpoints, and on every resume
  entry.
- Cancellation fails as a limit error with the guest-safe message
  `execution cancelled`.
- In addon mode, same-thread `AbortSignal` delivery cannot interrupt a native
  compute segment until control returns to Node. Explicit `Progress.cancel()`
  still aborts already suspended executions immediately.

## Default Policy

- Limits are enabled by default.
- Cancellation is cooperative and checked at defined execution points.
- If the live heap still exceeds configured limits after collection,
  over-budget execution fails with guest-safe runtime errors.
