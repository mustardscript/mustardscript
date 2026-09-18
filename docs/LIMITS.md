---
title: "Resource Limits"
description: "Instruction budgets, heap limits, call depth, and timeout controls"
category: "Language & Runtime"
order: 4
slug: "limits"
lastUpdated: "2026-04-13"
---

# Limits

`mustard` exposes a limits struct early so resource controls stay part of the
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
  execution boundaries only when recent allocation debt or current budget
  pressure makes collection worthwhile, and it still forces a collection before
  failing heap or allocation pressure, so unreachable cycles can be reclaimed
  without changing handle identities. Collections now subtract reclaimed cached
  accounting totals during sweep instead of rewalking the whole heap after
  every pass; full recounts are reserved for snapshot load and debug-only
  assertions.
- Snapshot load recomputes heap accounting once before policy is rebound so
  serialized inputs cannot bypass the configured heap and allocation budgets,
  and later restore-policy application reuses those verified totals instead of
  rewalking the whole heap again.
- Loaded snapshots do not get to keep serialized runtime limits. Resume policy
  must reassert explicit host limits, and restored execution fails closed if the
  snapshot is already over the host's configured heap, allocation, depth,
  outstanding-call, or instruction budgets.
- Outstanding host-call limits are enforced for async guest execution across
  queued and currently suspended host capability requests.
- Call-depth limits are enforced before each new guest frame is pushed, so
  recursive or deeply nested guest calls fail with a guest-safe limit error
  once the configured depth budget is exhausted.
- Native recursion has an additional fixed, shared budget of 256 units:
  each JSON property traversal or parse entry costs one, and each synchronous
  native-to-guest callback entry costs sixteen. Reentrant revivers, replacers,
  `toJSON`, and collection callbacks cannot reset this budget by starting a new
  helper. Exits refund their units, including errors. Exhaustion reports
  `native recursion depth limit exceeded` as a non-catchable limit error.
  Native builds use `stacker` at these guarded entries to reserve stack space
  on small threads and debug/ASan builds; the fixed recursion budget bounds
  stack-segment growth independently of guest heap limits. WebAssembly uses
  the same depth budget without native stack switching.
- Cooperative cancellation is implemented and checked before each instruction,
  before idle microtask or queued-host-call checkpoints, on every resume
  entry, and inside long-running native helper loops.
- Global RegExp helpers carry byte/scalar cursors forward and charge consumed
  search spans plus capture materialization, not the entire input for every
  match. Capture-index conversion scans each matched span once. Exhausted
  searches still charge the remaining span; each attempt checks cancellation.
- `Array.prototype.shift` moves slots in place and charges one native removal,
  without allocating a discarded splice-result array or copying guest values.
  The backing vector still performs a bounded native slot move; this is not a
  claim of constant-time physical queue storage.
- Native helper loops such as `Array.prototype.sort()` and `Object.keys()` now
  charge instruction budget explicitly instead of bypassing the guest budget
  inside opaque Rust work.
- `JSON.parse()`, `JSON.stringify()`, `Number.parseInt()`, and
  `Number.parseFloat()` also meter native helper work instead of bypassing the
  guest instruction budget inside long native scans.
- `JSON.parse()` and `JSON.stringify()` observe cancellation during large helper
  runs, and direct top-level string returns fail against the configured heap
  limit before those results cross the host boundary.
- Bare string values now receive the same heap-headroom check when they cross
  the structured boundary directly as capability results or direct
  `Progress.resume(...)` payloads, so hosts cannot bypass `heapLimitBytes` by
  returning or resuming with an unboxed large string.
- Structured host-boundary arrays now fail closed above 1,000,000 elements, so
  sparse-array length bombs cannot force unbounded synchronous traversal before
  runtime limits begin.
- Cancellation fails as a limit error with the guest-safe message
  `execution cancelled`.
- In addon mode, same-thread `AbortSignal` delivery cannot interrupt a native
  compute segment until control returns to Node. Explicit `Progress.cancel()`
  still aborts already suspended executions immediately.
- Already-aborted `AbortSignal`s short-circuit before Node begins structured
  boundary encoding for `run()` and `start()`.

## Default Policy

- Limits are enabled by default.
- Cancellation is cooperative and checked at defined execution points.
- If the live heap still exceeds configured limits after collection,
  over-budget execution fails with guest-safe runtime errors.
